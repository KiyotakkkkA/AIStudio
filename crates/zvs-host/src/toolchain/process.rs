use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};
use std::time::{Duration, Instant};

use regex::Regex;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;
use zvs_contract::{AppError, AppErrorCode};

use super::launch::LaunchSpec;
use crate::drivers::mcp::{default_spawn, kill_tree};
use crate::kernel::types::cancelled;

pub const SCRATCH_FAILED: &str = "Не удалось подготовить рабочую папку скрипта";
pub const PROCESS_TIMEOUT: Duration = Duration::from_secs(120);
pub const PROCESS_OUTPUT_BYTES: usize = 64 * 1024;
pub const SCRATCH_KEEP: usize = 16;
const DRAIN_GRACE: Duration = Duration::from_secs(2);
const DEFAULT_PATHEXT: &str = ".COM;.EXE;.BAT;.CMD";

static STREAM_FENCE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)<(/?)(stdout|stderr)").expect("a valid pattern"));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessLimits {
    pub timeout: Duration,
    pub output_bytes: usize,
}

impl Default for ProcessLimits {
    fn default() -> Self {
        Self {
            timeout: PROCESS_TIMEOUT,
            output_bytes: PROCESS_OUTPUT_BYTES,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessRun {
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub truncated: bool,
    pub stdout: String,
    pub stderr: String,
    pub duration: Duration,
}

impl ProcessRun {
    pub fn failed(&self) -> bool {
        self.timed_out || self.exit_code != Some(0)
    }

    pub fn duration_ms(&self) -> u64 {
        u64::try_from(self.duration.as_millis()).unwrap_or(u64::MAX)
    }
}

#[derive(Default)]
struct Capture {
    bytes: Vec<u8>,
    truncated: bool,
}

type SharedCapture = Arc<Mutex<Capture>>;

async fn drain(mut stream: impl AsyncRead + Unpin, capture: SharedCapture, limit: usize) {
    let mut buffer = [0_u8; 8192];
    loop {
        let read = match stream.read(&mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(read) => read,
        };
        let mut capture = capture.lock().unwrap_or_else(PoisonError::into_inner);
        let room = limit.saturating_sub(capture.bytes.len());
        if read > room {
            capture.truncated = true;
        }
        capture.bytes.extend_from_slice(&buffer[..read.min(room)]);
    }
}

fn taken(capture: &SharedCapture) -> (String, bool) {
    let capture = capture.lock().unwrap_or_else(PoisonError::into_inner);
    (
        String::from_utf8_lossy(&capture.bytes).into_owned(),
        capture.truncated,
    )
}

pub fn fenced(tag: &str, text: &str) -> String {
    let body = STREAM_FENCE.replace_all(text, "<$1 $2");
    format!("<{tag}>\n{body}\n</{tag}>")
}

pub fn process_text(run: &ProcessRun, noun: &str, limits: ProcessLimits) -> String {
    let code = run
        .exit_code
        .map_or_else(|| "none".to_owned(), |code| code.to_string());
    let mut lines = vec![format!("exit code: {code}")];
    if run.timed_out {
        lines.push(format!(
            "[the {noun} was stopped after {} seconds]",
            limits.timeout.as_secs()
        ));
    }
    if run.truncated {
        lines.push(format!(
            "[output truncated to {} bytes per stream]",
            limits.output_bytes
        ));
    }
    lines.push(fenced("stdout", &run.stdout));
    lines.push(fenced("stderr", &run.stderr));
    lines.join("\n")
}

pub fn prepare_scratch(root: &Path, run: &str) -> Result<PathBuf, AppError> {
    let failed = |_: std::io::Error| AppError::new(AppErrorCode::NativeError, SCRATCH_FAILED);
    let scratch = root.join(run);
    fs::create_dir_all(&scratch).map_err(failed)?;
    let mut others: Vec<(std::time::SystemTime, PathBuf)> = fs::read_dir(root)
        .map_err(failed)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() && *path != scratch)
        .filter_map(|path| {
            let modified = fs::metadata(&path).and_then(|meta| meta.modified()).ok()?;
            Some((modified, path))
        })
        .collect();
    others.sort_by(|left, right| right.0.cmp(&left.0));
    for (_, stale) in others.into_iter().skip(SCRATCH_KEEP.saturating_sub(1)) {
        let _ = fs::remove_dir_all(stale);
    }
    Ok(scratch)
}

pub fn path_dirs(current: Option<&OsString>) -> Vec<PathBuf> {
    current
        .map(|current| {
            std::env::split_paths(current)
                .filter(|entry| !entry.as_os_str().is_empty())
                .collect()
        })
        .unwrap_or_default()
}

pub fn joined_path(dirs: &[PathBuf]) -> String {
    std::env::join_paths(dirs).map_or_else(
        |_| {
            dirs.iter()
                .map(|dir| dir.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join(if cfg!(windows) { ";" } else { ":" })
        },
        |joined| joined.to_string_lossy().into_owned(),
    )
}

fn extensions(windows: bool, pathext: Option<&str>) -> Vec<String> {
    if !windows {
        return Vec::new();
    }
    pathext
        .unwrap_or(DEFAULT_PATHEXT)
        .split(';')
        .map(str::trim)
        .filter(|extension| extension.starts_with('.'))
        .map(str::to_ascii_lowercase)
        .collect()
}

fn candidates(base: &Path, windows: bool, extensions: &[String]) -> Vec<PathBuf> {
    let has_extension = base.extension().is_some_and(|extension| {
        let extension = format!(".{}", extension.to_string_lossy().to_ascii_lowercase());
        extensions.contains(&extension)
    });
    if !windows || has_extension {
        return vec![base.to_path_buf()];
    }
    extensions
        .iter()
        .map(|extension| {
            let mut name = base.as_os_str().to_owned();
            name.push(extension);
            PathBuf::from(name)
        })
        .collect()
}

pub fn find_program(
    name: &str,
    dirs: &[PathBuf],
    cwd: &Path,
    pathext: Option<&str>,
    exists: &dyn Fn(&Path) -> bool,
) -> Option<PathBuf> {
    let windows = cfg!(windows);
    let extensions = extensions(windows, pathext);
    let named = Path::new(name);
    if named.is_absolute() || name.contains(['/', '\\']) {
        return candidates(&cwd.join(named), windows, &extensions)
            .into_iter()
            .find(|candidate| exists(candidate));
    }
    dirs.iter()
        .flat_map(|dir| candidates(&dir.join(named), windows, &extensions))
        .find(|candidate| exists(candidate))
}

enum Ending {
    Exited(Option<i32>),
    TimedOut,
    Cancelled,
}

pub async fn run_process(
    spec: &LaunchSpec,
    stdin: Option<&str>,
    limits: ProcessLimits,
    cancel: &CancellationToken,
    not_started: &str,
) -> Result<ProcessRun, AppError> {
    if cancel.is_cancelled() {
        return Err(cancelled());
    }
    let started = Instant::now();
    let mut child = default_spawn(spec).map_err(|error| {
        AppError::new(AppErrorCode::NativeError, not_started)
            .with_detail("reason", error.to_string())
    })?;
    let limit = limits.output_bytes;
    let stdout = SharedCapture::default();
    let stderr = SharedCapture::default();
    let mut readers = Vec::new();
    if let Some(pipe) = child.stdout.take() {
        readers.push(tokio::spawn(drain(pipe, Arc::clone(&stdout), limit)));
    }
    if let Some(pipe) = child.stderr.take() {
        readers.push(tokio::spawn(drain(pipe, Arc::clone(&stderr), limit)));
    }
    if let Some(mut pipe) = child.stdin.take() {
        let text = stdin.unwrap_or_default().to_owned();
        tokio::spawn(async move {
            if !text.is_empty() {
                let _ = pipe.write_all(text.as_bytes()).await;
            }
            let _ = pipe.shutdown().await;
        });
    }
    let ending = tokio::select! {
        status = child.wait() => Ending::Exited(status.ok().and_then(|status| status.code())),
        () = tokio::time::sleep(limits.timeout) => Ending::TimedOut,
        () = cancel.cancelled() => Ending::Cancelled,
    };
    if !matches!(ending, Ending::Exited(_)) {
        kill_tree(&mut child).await;
        let _ = child.wait().await;
    }
    for reader in readers {
        let handle = reader.abort_handle();
        if tokio::time::timeout(DRAIN_GRACE, reader).await.is_err() {
            handle.abort();
        }
    }
    let exit_code = match ending {
        Ending::Cancelled => return Err(cancelled()),
        Ending::TimedOut => None,
        Ending::Exited(code) => code,
    };
    let (stdout, stdout_truncated) = taken(&stdout);
    let (stderr, stderr_truncated) = taken(&stderr);
    Ok(ProcessRun {
        exit_code,
        timed_out: matches!(ending, Ending::TimedOut),
        truncated: stdout_truncated || stderr_truncated,
        stdout,
        stderr,
        duration: started.elapsed(),
    })
}
