use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use async_trait::async_trait;
use regex::Regex;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use zvs_contract::{AppError, AppErrorCode};

use crate::platform::audio::SAMPLE_RATE;

pub const RECOGNITION_FAILED: &str = "Не удалось распознать речь";
pub const RECOGNITION_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const CLI_NAMES: [&str; 2] = ["whisper-cli.exe", "whisper-cli"];
const STDERR_TAIL: usize = 2000;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

static NON_SPEECH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[[^\]]*\]").expect("a valid pattern"));

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscribeRequest {
    pub executable: PathBuf,
    pub model: PathBuf,
    pub audio: PathBuf,
    pub output_base: PathBuf,
    pub threads: usize,
}

impl TranscribeRequest {
    pub fn argv(&self) -> Vec<String> {
        vec![
            "-m".to_owned(),
            self.model.to_string_lossy().into_owned(),
            "-f".to_owned(),
            self.audio.to_string_lossy().into_owned(),
            "-l".to_owned(),
            "auto".to_owned(),
            "-t".to_owned(),
            self.threads.to_string(),
            "-nt".to_owned(),
            "-np".to_owned(),
            "-otxt".to_owned(),
            "-of".to_owned(),
            self.output_base.to_string_lossy().into_owned(),
        ]
    }

    pub fn transcript_path(&self) -> PathBuf {
        let mut path = self.output_base.clone().into_os_string();
        path.push(".txt");
        PathBuf::from(path)
    }
}

#[async_trait]
pub trait SpeechRecognizer: Send + Sync {
    async fn transcribe(&self, request: &TranscribeRequest) -> Result<String, AppError>;
}

pub type SharedSpeechRecognizer = Arc<dyn SpeechRecognizer>;

pub fn wav_bytes(samples: &[i16]) -> Vec<u8> {
    let data_len = u32::try_from(samples.len() * 2).unwrap_or(u32::MAX - 36);
    let mut out = Vec::with_capacity(44 + samples.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16_u32.to_le_bytes());
    out.extend_from_slice(&1_u16.to_le_bytes());
    out.extend_from_slice(&1_u16.to_le_bytes());
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    out.extend_from_slice(&2_u16.to_le_bytes());
    out.extend_from_slice(&16_u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}

pub fn clean_transcript(raw: &str) -> String {
    NON_SPEECH
        .replace_all(raw, " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn find_cli(root: &Path, depth: usize) -> Option<PathBuf> {
    if depth > 4 {
        return None;
    }
    let mut entries: Vec<std::fs::DirEntry> = std::fs::read_dir(root)
        .map(|entries| entries.flatten().collect())
        .unwrap_or_default();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    let found = entries.iter().find(|entry| {
        entry.file_type().is_ok_and(|kind| kind.is_file())
            && CLI_NAMES.contains(&entry.file_name().to_string_lossy().to_lowercase().as_str())
    });
    if let Some(found) = found {
        return Some(found.path());
    }
    entries
        .iter()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .find_map(|entry| find_cli(&entry.path(), depth + 1))
}

fn failure(reason: impl Into<String>) -> AppError {
    AppError::new(AppErrorCode::NativeError, RECOGNITION_FAILED)
        .with_detail("reason", reason.into())
}

fn tail(text: &str) -> String {
    let start = text.len().saturating_sub(STDERR_TAIL);
    let start = (start..text.len())
        .find(|index| text.is_char_boundary(*index))
        .unwrap_or(text.len());
    text[start..].to_owned()
}

#[derive(Debug, Clone, Copy, Default)]
pub struct WhisperCli;

#[async_trait]
impl SpeechRecognizer for WhisperCli {
    async fn transcribe(&self, request: &TranscribeRequest) -> Result<String, AppError> {
        let mut command = Command::new(&request.executable);
        command
            .args(request.argv())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if let Some(directory) = request.executable.parent() {
            command.current_dir(directory);
        }
        #[cfg(windows)]
        command.creation_flags(CREATE_NO_WINDOW);
        let mut child = command
            .spawn()
            .map_err(|error| failure(format!("spawn: {error}")))?;
        let mut stderr = child.stderr.take();
        let run = async {
            let mut captured = String::new();
            if let Some(stderr) = stderr.as_mut() {
                let _ = stderr.read_to_string(&mut captured).await;
            }
            let status = child.wait().await;
            (status, captured)
        };
        let (status, captured) = tokio::time::timeout(RECOGNITION_TIMEOUT, run)
            .await
            .map_err(|_| failure("timed out"))?;
        let status = status.map_err(|error| failure(format!("wait: {error}")))?;
        if !status.success() {
            return Err(failure(format!("exit {status}: {}", tail(&captured))));
        }
        let transcript = request.transcript_path();
        let raw = tokio::fs::read(&transcript)
            .await
            .map_err(|error| failure(format!("transcript: {error}")))?;
        let _ = tokio::fs::remove_file(&transcript).await;
        Ok(clean_transcript(&String::from_utf8_lossy(&raw)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wav_header_describes_mono_sixteen_bit_pcm() {
        let bytes = wav_bytes(&[0, 1, -1]);
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(bytes.len(), 44 + 6);
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().expect("4")), 42);
        assert_eq!(u16::from_le_bytes(bytes[22..24].try_into().expect("2")), 1);
        assert_eq!(
            u32::from_le_bytes(bytes[24..28].try_into().expect("4")),
            SAMPLE_RATE
        );
        assert_eq!(u32::from_le_bytes(bytes[40..44].try_into().expect("4")), 6);
        assert_eq!(&bytes[44..46], &0_i16.to_le_bytes());
    }

    #[test]
    fn non_speech_markers_and_line_breaks_are_dropped() {
        assert_eq!(
            clean_transcript(" [BLANK_AUDIO]\n Привет.\n Как дела? [Музыка]\n"),
            "Привет. Как дела?"
        );
        assert_eq!(clean_transcript("[BLANK_AUDIO]"), "");
    }

    #[test]
    fn the_command_line_writes_a_plain_transcript_next_to_the_audio() {
        let request = TranscribeRequest {
            executable: PathBuf::from("whisper-cli.exe"),
            model: PathBuf::from("model.bin"),
            audio: PathBuf::from("clip.wav"),
            output_base: PathBuf::from("clip"),
            threads: 4,
        };
        let argv = request.argv();
        assert!(argv.windows(2).any(|pair| pair == ["-m", "model.bin"]));
        assert!(argv.windows(2).any(|pair| pair == ["-f", "clip.wav"]));
        assert!(argv.windows(2).any(|pair| pair == ["-t", "4"]));
        assert!(argv.iter().any(|arg| arg == "-otxt"));
        assert_eq!(request.transcript_path(), PathBuf::from("clip.txt"));
    }

    #[test]
    fn the_cli_is_found_inside_the_unpacked_release() {
        let root = tempfile::tempdir().expect("dir");
        let release = root.path().join("Release");
        std::fs::create_dir_all(&release).expect("creates");
        std::fs::write(release.join("whisper-server.exe"), b"").expect("writes");
        std::fs::write(release.join("whisper-cli.exe"), b"").expect("writes");
        assert_eq!(
            find_cli(root.path(), 0),
            Some(release.join("whisper-cli.exe"))
        );
        assert_eq!(find_cli(&root.path().join("absent"), 0), None);
    }
}
