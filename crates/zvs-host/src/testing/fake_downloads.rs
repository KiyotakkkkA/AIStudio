use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use async_trait::async_trait;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::sync::{Notify, watch};
use zvs_contract::{AppError, AppErrorCode};

use crate::downloads::DiskProbe;
use crate::downloads::disk::directory_size;
use crate::drivers::sidecar::{CANCELLED_MESSAGE, JobProgress, JobRunOptions, SidecarJobs};
use crate::platform::disk::FreeSpace;

pub struct FixedDiskProbe {
    pub total: u64,
    pub free: AtomicU64,
}

impl FixedDiskProbe {
    pub fn shared(total: u64, free: u64) -> Arc<Self> {
        Arc::new(Self {
            total,
            free: AtomicU64::new(free),
        })
    }

    pub fn set_free(&self, free: u64) {
        self.free.store(free, Ordering::SeqCst);
    }
}

#[async_trait]
impl DiskProbe for FixedDiskProbe {
    async fn free(&self, _path: &Path) -> Result<FreeSpace, AppError> {
        Ok(FreeSpace {
            total_bytes: self.total,
            free_bytes: self.free.load(Ordering::SeqCst),
        })
    }

    async fn size(&self, path: &Path) -> u64 {
        directory_size(path)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadAttempt {
    pub url: String,
    pub target_path: String,
    pub resumed_from: u64,
}

#[derive(Debug, Clone)]
struct Resource {
    bytes: Vec<u8>,
    chunk_size: Option<usize>,
}

#[derive(Default)]
struct State {
    resources: HashMap<String, Resource>,
    attempts: Vec<DownloadAttempt>,
    gates: HashMap<String, watch::Sender<bool>>,
}

#[derive(Default)]
pub struct FakeDownloadJobs {
    state: Mutex<State>,
    started: Notify,
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut text, byte| {
            let _ = write!(text, "{byte:02x}");
            text
        })
}

fn part_of(target: &str) -> String {
    format!("{target}.part")
}

fn size_of(path: &str) -> u64 {
    std::fs::metadata(path).map_or(0, |metadata| metadata.len())
}

fn failure(error: &std::io::Error) -> AppError {
    AppError::new(AppErrorCode::Unknown, error.to_string())
}

impl FakeDownloadJobs {
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn serve(&self, url: &str, bytes: &[u8], chunk_size: Option<usize>) {
        self.state().resources.insert(
            url.to_owned(),
            Resource {
                bytes: bytes.to_vec(),
                chunk_size,
            },
        );
    }

    pub fn hold(&self, url: &str) {
        self.state()
            .gates
            .insert(url.to_owned(), watch::channel(false).0);
    }

    pub fn release(&self, url: &str) {
        if let Some(gate) = self.state().gates.remove(url) {
            let _ = gate.send(true);
        }
    }

    pub fn attempts(&self) -> Vec<DownloadAttempt> {
        self.state().attempts.clone()
    }

    pub async fn started(&self, count: usize) {
        loop {
            let notified = self.started.notified();
            if self.state().attempts.len() >= count {
                return;
            }
            notified.await;
        }
    }

    fn gate(&self, url: &str) -> Option<watch::Receiver<bool>> {
        self.state().gates.get(url).map(watch::Sender::subscribe)
    }
}

#[async_trait]
impl SidecarJobs for FakeDownloadJobs {
    async fn run(
        &self,
        job: &str,
        params: Value,
        options: JobRunOptions,
    ) -> Result<Value, AppError> {
        assert_eq!(job, "job.download", "unexpected job");
        let url = params["url"].as_str().unwrap_or_default().to_owned();
        let target = params["targetPath"].as_str().unwrap_or_default().to_owned();
        let checksum = params
            .get("checksum")
            .and_then(|checksum| checksum["value"].as_str())
            .map(str::to_lowercase);
        let resource = self.state().resources.get(&url).cloned().ok_or_else(|| {
            AppError::new(AppErrorCode::NotFound, format!("No resource at {url}"))
        })?;
        let part = part_of(&target);
        if let Some(parent) = Path::new(&target).parent() {
            std::fs::create_dir_all(parent).map_err(|error| failure(&error))?;
        }
        let resumed_from = size_of(&part);
        let attempt = DownloadAttempt {
            url: url.clone(),
            target_path: target.clone(),
            resumed_from,
        };
        let mut announced = false;
        let announce = |announced: &mut bool| {
            if !*announced {
                *announced = true;
                self.state().attempts.push(attempt.clone());
                self.started.notify_waiters();
            }
        };

        let total = resource.bytes.len() as u64;
        let progress = |done: u64, rate: f64| {
            if let Some(sink) = &options.on_progress {
                sink(JobProgress {
                    done,
                    total,
                    rate: Some(rate),
                    message: None,
                });
            }
        };
        let cancelled = || {
            options
                .cancel
                .as_ref()
                .is_some_and(tokio_util::sync::CancellationToken::is_cancelled)
        };
        let mut done = usize::try_from(resumed_from).unwrap_or(usize::MAX);
        progress(resumed_from, 0.0);
        let step = resource.chunk_size.unwrap_or(resource.bytes.len()).max(1);
        while done < resource.bytes.len() {
            if cancelled() {
                announce(&mut announced);
                return Err(AppError::new(AppErrorCode::RunCancelled, CANCELLED_MESSAGE));
            }
            let next = resource.bytes.len().min(done + step);
            std::fs::write(&part, &resource.bytes[..next]).map_err(|error| failure(&error))?;
            done = next;
            progress(done as u64, 1_000_000.0);
            announce(&mut announced);
            if let Some(mut gate) = self.gate(&url) {
                let cancel = options.cancel.clone().unwrap_or_default();
                tokio::select! {
                    _ = gate.wait_for(|open| *open) => {}
                    () = cancel.cancelled() => {}
                }
            }
        }
        announce(&mut announced);
        if cancelled() {
            return Err(AppError::new(AppErrorCode::RunCancelled, CANCELLED_MESSAGE));
        }
        if let Some(expected) = checksum {
            let actual = sha256_hex(&std::fs::read(&part).map_err(|error| failure(&error))?);
            if actual != expected {
                let _ = std::fs::remove_file(&part);
                return Err(AppError::new(
                    AppErrorCode::ValidationFailed,
                    format!("Checksum mismatch: got {actual}"),
                ));
            }
        }
        std::fs::rename(&part, &target).map_err(|error| failure(&error))?;
        Ok(json!({
            "path": target,
            "bytes": total,
            "resumedFrom": resumed_from,
            "resumable": true,
            "checksum": null,
        }))
    }
}
