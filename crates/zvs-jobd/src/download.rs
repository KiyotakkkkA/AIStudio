use std::{
    fmt::Write as _,
    io::ErrorKind,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use futures::StreamExt;
use reqwest::{
    Client, StatusCode,
    header::{ACCEPT_RANGES, CONTENT_LENGTH, RANGE},
};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::{
    fs::{self, File, OpenOptions},
    io::{AsyncReadExt, AsyncWriteExt, BufWriter},
};
use zvs_core::{Error, Result};

use crate::jobs::JobContext;

pub const DOWNLOAD_JOB: &str = "job.download";
pub const PART_SUFFIX: &str = ".part";

const HASH_BUFFER: usize = 1024 * 1024;
const RATE_HALF_LIFE_SECS: f64 = 3.0;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ChecksumAlgorithm {
    Sha256,
    Blake3,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Checksum {
    pub algorithm: ChecksumAlgorithm,
    pub value: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DownloadParams {
    pub url: String,
    pub target_path: String,
    #[serde(default)]
    pub size_bytes: Option<u64>,
    #[serde(default)]
    pub checksum: Option<Checksum>,
    #[serde(default)]
    pub connect_timeout_ms: Option<u64>,
}

pub fn part_path(target: &Path) -> PathBuf {
    let mut name = target.as_os_str().to_os_string();
    name.push(PART_SUFFIX);
    PathBuf::from(name)
}

pub async fn download(params: Value, context: &JobContext) -> Result<Value> {
    let params: DownloadParams = serde_json::from_value(params).map_err(|error| {
        Error::InvalidInput(format!("Invalid {DOWNLOAD_JOB} parameters: {error}"))
    })?;
    if !params.url.starts_with("http://") && !params.url.starts_with("https://") {
        return Err(Error::InvalidInput(format!(
            "{DOWNLOAD_JOB} only fetches http and https URLs"
        )));
    }
    let target = PathBuf::from(&params.target_path);
    let parent = target
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| {
            Error::InvalidInput("The download target needs a parent directory".into())
        })?;
    fs::create_dir_all(parent).await?;

    let part = part_path(&target);
    let resumed_from = match fs::metadata(&part).await {
        Ok(metadata) => metadata.len(),
        Err(error) if error.kind() == ErrorKind::NotFound => 0,
        Err(error) => return Err(error.into()),
    };

    let transfer = fetch(&params, &part, resumed_from, context).await?;
    let checksum = match &params.checksum {
        None => None,
        Some(expected) => Some(verify(&part, expected, context).await?),
    };
    replace(&part, &target).await?;

    let total = fs::metadata(&target).await?.len();
    context.progress_at(total, total, Some(0.0), None);
    Ok(json!({
        "path": target.to_string_lossy(),
        "bytes": total,
        "resumedFrom": transfer.resumed_from,
        "resumable": transfer.resumable,
        "checksum": checksum,
    }))
}

struct Transfer {
    /// Bytes already on disk that the server let us keep. Zero when the range was declined.
    resumed_from: u64,
    resumable: bool,
}

/// Appends the remainder of the resource to `part`, restarting from zero when the server
/// declines the range request. Leaves the partial file in place on cancellation so the next
/// attempt resumes instead of starting over.
async fn fetch(
    params: &DownloadParams,
    part: &Path,
    offset: u64,
    context: &JobContext,
) -> Result<Transfer> {
    let mut builder = Client::builder();
    if let Some(timeout) = params.connect_timeout_ms {
        builder = builder.connect_timeout(Duration::from_millis(timeout));
    }
    let client = builder
        .build()
        .map_err(|error| Error::Backend(error.to_string()))?;

    let mut request = client.get(&params.url);
    if offset > 0 {
        request = request.header(RANGE, format!("bytes={offset}-"));
    }
    let response = request
        .send()
        .await
        .map_err(|error| Error::Backend(format!("Could not reach {}: {error}", params.url)))?;

    let status = response.status();
    if offset > 0 && status == StatusCode::RANGE_NOT_SATISFIABLE {
        // Everything the resource has is already on disk.
        return Ok(Transfer {
            resumed_from: offset,
            resumable: true,
        });
    }
    if !status.is_success() {
        return Err(Error::Backend(format!("{} answered {status}", params.url)));
    }

    let ranged = status == StatusCode::PARTIAL_CONTENT;
    if offset > 0 && !ranged {
        eprintln!(
            "zvs-jobd: {} ignored the range request; restarting from zero",
            params.url
        );
    }
    let start = if ranged { offset } else { 0 };
    let total = response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .map(|length| start + length)
        .or(params.size_bytes)
        .unwrap_or(0);
    let resumable = ranged || response.headers().contains_key(ACCEPT_RANGES);

    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .append(ranged)
        .truncate(!ranged)
        .open(part)
        .await?;
    let mut writer = BufWriter::new(file);

    let mut done = start;
    let mut rate = Rate::new(Instant::now());
    context.progress_at(done, total.max(done), Some(0.0), None);

    let mut stream = response.bytes_stream();
    loop {
        let next = tokio::select! {
            () = context.token().cancelled() => {
                settle(writer).await?;
                return Err(Error::Cancelled);
            }
            next = stream.next() => next,
        };
        let Some(chunk) = next else { break };
        let chunk = chunk.map_err(|error| Error::Backend(format!("Transfer failed: {error}")))?;
        writer.write_all(&chunk).await?;
        done += chunk.len() as u64;
        let smoothed = rate.sample(chunk.len() as u64, Instant::now());
        context.progress_at(done, total.max(done), Some(smoothed), None);
    }
    settle(writer).await?;
    Ok(Transfer {
        resumed_from: start,
        resumable,
    })
}

async fn settle(mut writer: BufWriter<File>) -> Result<()> {
    writer.flush().await?;
    writer.into_inner().sync_all().await?;
    Ok(())
}

/// Hashes the completed partial file and deletes it when it does not match, so a corrupt
/// transfer never lands at the target and is never resumed from.
async fn verify(part: &Path, expected: &Checksum, context: &JobContext) -> Result<String> {
    let actual = hash(part, expected.algorithm, context).await?;
    if !actual.eq_ignore_ascii_case(expected.value.trim()) {
        fs::remove_file(part).await.ok();
        return Err(Error::InvalidInput(format!(
            "Checksum mismatch: expected {}, got {actual}",
            expected.value
        )));
    }
    Ok(actual)
}

async fn replace(part: &Path, target: &Path) -> Result<()> {
    match fs::rename(part, target).await {
        Ok(()) => Ok(()),
        // Windows refuses a rename onto an existing file; re-downloading replaces what is there.
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::AlreadyExists | ErrorKind::PermissionDenied
            ) =>
        {
            fs::remove_file(target).await.ok();
            fs::rename(part, target).await?;
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}

enum Hasher {
    Sha256(Box<sha2::Sha256>),
    Blake3(Box<blake3::Hasher>),
}

async fn hash(path: &Path, algorithm: ChecksumAlgorithm, context: &JobContext) -> Result<String> {
    use sha2::Digest as _;

    let mut hasher = match algorithm {
        ChecksumAlgorithm::Sha256 => Hasher::Sha256(Box::default()),
        ChecksumAlgorithm::Blake3 => Hasher::Blake3(Box::default()),
    };
    let mut file = File::open(path).await?;
    let mut buffer = vec![0_u8; HASH_BUFFER];
    loop {
        if context.token().is_cancelled() {
            return Err(Error::Cancelled);
        }
        let read = file.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        match &mut hasher {
            Hasher::Sha256(sha) => sha.update(&buffer[..read]),
            Hasher::Blake3(blake) => {
                blake.update(&buffer[..read]);
            }
        }
    }
    Ok(match hasher {
        Hasher::Sha256(sha) => to_hex(sha.finalize().as_slice()),
        Hasher::Blake3(blake) => blake.finalize().to_hex().to_string(),
    })
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut hex, byte| {
        let _ = write!(hex, "{byte:02x}");
        hex
    })
}

/// An exponentially weighted transfer rate. The instantaneous rate swings wildly between
/// chunks and reads as a broken progress bar, so every sample decays towards the running mean.
struct Rate {
    value: f64,
    at: Instant,
}

impl Rate {
    fn new(at: Instant) -> Self {
        Self { value: 0.0, at }
    }

    fn sample(&mut self, bytes: u64, now: Instant) -> f64 {
        let elapsed = now.duration_since(self.at).as_secs_f64();
        if elapsed <= 0.0 {
            return self.value;
        }
        self.at = now;
        let instant = bytes as f64 / elapsed;
        let weight = 1.0 - (-elapsed / RATE_HALF_LIFE_SECS).exp();
        self.value += (instant - self.value) * weight;
        self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_partial_file_sits_beside_the_target() {
        assert_eq!(
            part_path(Path::new("/models/a.gguf")),
            PathBuf::from("/models/a.gguf.part")
        );
    }

    #[test]
    fn the_smoothed_rate_climbs_towards_the_instantaneous_one() {
        let start = Instant::now();
        let mut rate = Rate::new(start);
        let first = rate.sample(1_000_000, start + Duration::from_secs(1));
        let second = rate.sample(1_000_000, start + Duration::from_secs(2));
        assert!(first > 0.0 && first < 1_000_000.0);
        assert!(second > first && second < 1_000_000.0);
    }

    #[test]
    fn hex_encoding_pads_every_byte() {
        assert_eq!(to_hex(&[0x00, 0x0f, 0xff]), "000fff");
    }
}
