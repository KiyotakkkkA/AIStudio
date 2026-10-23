use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::CancellationToken;
use crate::error::{Error, Result};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
pub const DEFAULT_CONVERT_TIMEOUT: Duration = Duration::from_secs(10 * 60);
pub const DEFAULT_POLL: Duration = Duration::from_millis(100);

pub trait OfficeConverter: Send + Sync {
    fn convert_to_pdf(
        &self,
        source: &Path,
        work_dir: &Path,
        cancel: &CancellationToken,
    ) -> Result<PathBuf>;
}

#[derive(Debug, Clone)]
pub struct SofficeConverter {
    pub executable: PathBuf,
    pub timeout: Duration,
    pub poll: Duration,
    pub extra_args: Vec<String>,
}

impl SofficeConverter {
    pub fn new(executable: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
            timeout: DEFAULT_CONVERT_TIMEOUT,
            poll: DEFAULT_POLL,
            extra_args: Vec::new(),
        }
    }

    pub fn arguments(&self, source: &Path, out_dir: &Path, profile: &Path) -> Vec<String> {
        let mut args = vec![
            "--headless".to_owned(),
            "--norestore".to_owned(),
            "--nolockcheck".to_owned(),
            "--nodefault".to_owned(),
            "--nologo".to_owned(),
            format!("-env:UserInstallation={}", file_url(profile)),
            "--convert-to".to_owned(),
            "pdf".to_owned(),
            "--outdir".to_owned(),
            out_dir.to_string_lossy().into_owned(),
        ];
        args.extend(self.extra_args.iter().cloned());
        args.push(source.to_string_lossy().into_owned());
        args
    }
}

pub fn file_url(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let mut encoded = String::with_capacity(text.len() + 8);
    for byte in text.bytes() {
        let keep =
            byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b':' | b'-' | b'_' | b'.' | b'~');
        if keep {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    if encoded.starts_with('/') {
        format!("file://{encoded}")
    } else {
        format!("file:///{encoded}")
    }
}

pub fn converted_name(source: &Path) -> Result<String> {
    let stem = source
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .filter(|stem| !stem.is_empty())
        .ok_or_else(|| Error::InvalidInput("The source has no file name".to_owned()))?;
    Ok(format!("{stem}.pdf"))
}

fn kill_tree(child: &mut Child) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = Command::new("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

impl OfficeConverter for SofficeConverter {
    fn convert_to_pdf(
        &self,
        source: &Path,
        work_dir: &Path,
        cancel: &CancellationToken,
    ) -> Result<PathBuf> {
        crate::error::check_cancelled(cancel)?;
        if !source.is_file() {
            return Err(Error::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "The source document does not exist",
            )));
        }
        let name = converted_name(source)?;
        let out_dir = work_dir.join("out");
        let profile = work_dir.join("profile");
        std::fs::create_dir_all(&out_dir)?;
        std::fs::create_dir_all(&profile)?;
        let mut command = Command::new(&self.executable);
        command
            .args(self.arguments(source, &out_dir, &profile))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = command
            .spawn()
            .map_err(|error| Error::Backend(format!("The converter could not start: {error}")))?;
        let deadline = Instant::now() + self.timeout;
        let status = loop {
            if cancel.is_cancelled() {
                kill_tree(&mut child);
                return Err(Error::Cancelled);
            }
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if Instant::now() >= deadline {
                kill_tree(&mut child);
                return Err(Error::LimitExceeded(format!(
                    "The conversion did not finish within {} s",
                    self.timeout.as_secs()
                )));
            }
            std::thread::sleep(self.poll);
        };
        let output = out_dir.join(name);
        if !status.success() || !output.is_file() {
            return Err(Error::Unsupported(format!(
                "The converter produced no PDF (exit code {})",
                status
                    .code()
                    .map_or_else(|| "?".to_owned(), |code| code.to_string())
            )));
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work() -> tempfile::TempDir {
        tempfile::tempdir().expect("a temporary directory")
    }

    fn sleeper(seconds: u64) -> SofficeConverter {
        let mut converter = if cfg!(windows) {
            let mut converter = SofficeConverter::new("powershell");
            converter.extra_args = vec![
                "-NoProfile".to_owned(),
                "-Command".to_owned(),
                format!("Start-Sleep -Seconds {seconds}"),
            ];
            converter
        } else {
            let mut converter = SofficeConverter::new("sh");
            converter.extra_args = vec!["-c".to_owned(), format!("sleep {seconds}")];
            converter
        };
        converter.poll = Duration::from_millis(20);
        converter
    }

    #[test]
    fn profiles_are_passed_as_percent_encoded_file_urls() {
        assert_eq!(
            file_url(Path::new("C:\\Users\\Иван\\x y")),
            "file:///C:/Users/%D0%98%D0%B2%D0%B0%D0%BD/x%20y"
        );
        assert_eq!(file_url(Path::new("/tmp/p")), "file:///tmp/p");
    }

    #[test]
    fn the_command_line_converts_headless_into_the_out_dir_with_its_own_profile() {
        let converter = SofficeConverter::new("soffice");
        let args = converter.arguments(Path::new("a.docx"), Path::new("out"), Path::new("/p"));
        assert_eq!(args.first().map(String::as_str), Some("--headless"));
        assert!(args.contains(&"-env:UserInstallation=file:///p".to_owned()));
        assert_eq!(args.last().map(String::as_str), Some("a.docx"));
        assert_eq!(
            converted_name(Path::new("C:/x/Отчёт.docx")).unwrap(),
            "Отчёт.pdf"
        );
    }

    #[test]
    fn a_hanging_converter_is_killed_at_the_deadline() {
        let directory = work();
        let source = directory.path().join("a.docx");
        std::fs::write(&source, b"PK").unwrap();
        let mut converter = sleeper(30);
        converter.timeout = Duration::from_millis(300);
        let started = Instant::now();
        let error = converter
            .convert_to_pdf(
                &source,
                &directory.path().join("work"),
                &CancellationToken::new(),
            )
            .unwrap_err();
        assert_eq!(error.code(), "LIMIT_EXCEEDED");
        assert!(started.elapsed() < Duration::from_secs(20));
    }

    #[test]
    fn cancellation_kills_the_converter() {
        let directory = work();
        let source = directory.path().join("a.docx");
        std::fs::write(&source, b"PK").unwrap();
        let converter = sleeper(30);
        let token = CancellationToken::new();
        let cancel = token.clone();
        let timer = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            cancel.cancel();
        });
        let error = converter
            .convert_to_pdf(&source, &directory.path().join("work"), &token)
            .unwrap_err();
        timer.join().unwrap();
        assert_eq!(error.code(), "RUN_CANCELLED");
    }

    #[test]
    fn a_converter_that_writes_nothing_reports_an_unsupported_document() {
        let directory = work();
        let source = directory.path().join("a.docx");
        std::fs::write(&source, b"PK").unwrap();
        let error = sleeper(0)
            .convert_to_pdf(
                &source,
                &directory.path().join("work"),
                &CancellationToken::new(),
            )
            .unwrap_err();
        assert_eq!(error.code(), "UNSUPPORTED_FORMAT");
        let missing = SofficeConverter::new("soffice")
            .convert_to_pdf(
                &directory.path().join("none.docx"),
                &directory.path().join("work"),
                &CancellationToken::new(),
            )
            .unwrap_err();
        assert_eq!(missing.code(), "NOT_FOUND");
    }
}
