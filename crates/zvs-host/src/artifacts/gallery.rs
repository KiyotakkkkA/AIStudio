use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use async_trait::async_trait;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use sha2::{Digest, Sha256};
use zvs_contract::artifacts::{
    ARTIFACT_FORMAT_MAX, ARTIFACT_NAME_MAX, GALLERY_ITEMS_MAX, GALLERY_OWNER_MAX,
};
use zvs_contract::{
    AppError, AppErrorCode, GalleryItemDto, GalleryKind, GalleryListDto, GalleryThumbnailDto,
};

use super::{ArtifactStore, DASHBOARDS, is_artifact};
use crate::platform::external::SharedFileLauncher;
use crate::platform::long_path::extended;

pub const REMOVE_FAILED: &str = "Не удалось удалить файл";
pub const CAPTURE_FAILED: &str = "Не удалось подготовить миниатюру";
pub const PREVIEW_QUERY: &str = "preview";
const IMAGE_BYTES_MAX: u64 = 8 * 1024 * 1024;
const SVG_BYTES_MAX: u64 = 2 * 1024 * 1024;
const PDF_BYTES_MAX: u64 = 16 * 1024 * 1024;
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(30);
const CAPTURE_SIZE: &str = "1280,800";
const CHROMIUM_FAMILY: &[&str] = &[
    "msedge", "chrome", "chromium", "brave", "yandex", "vivaldi", "opera",
];

#[async_trait]
pub trait PageCapture: Send + Sync {
    async fn capture(&self, url: &str, target: &Path) -> Result<(), AppError>;
}

pub type SharedPageCapture = Arc<dyn PageCapture>;

pub struct HeadlessCapture {
    launcher: SharedFileLauncher,
    profile: PathBuf,
}

impl HeadlessCapture {
    pub fn new(launcher: SharedFileLauncher, profile: PathBuf) -> Self {
        Self { launcher, profile }
    }

    fn program(&self) -> Option<PathBuf> {
        let mut found: Vec<PathBuf> = self
            .launcher
            .browsers()
            .into_iter()
            .map(|browser| browser.program)
            .filter(|program| {
                let name = program
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().to_ascii_lowercase())
                    .unwrap_or_default();
                CHROMIUM_FAMILY.iter().any(|family| name.contains(family))
            })
            .collect();
        found.sort_by_key(|program| {
            !program
                .to_string_lossy()
                .to_ascii_lowercase()
                .contains("msedge")
        });
        found.into_iter().next()
    }
}

#[async_trait]
impl PageCapture for HeadlessCapture {
    async fn capture(&self, url: &str, target: &Path) -> Result<(), AppError> {
        let program = self
            .program()
            .ok_or_else(|| AppError::new(AppErrorCode::NotFound, CAPTURE_FAILED))?;
        let _ = std::fs::create_dir_all(&self.profile);
        let mut command = tokio::process::Command::new(program);
        command
            .args([
                "--headless=new",
                "--disable-gpu",
                "--hide-scrollbars",
                "--mute-audio",
                "--no-first-run",
                "--no-default-browser-check",
                "--disable-extensions",
                "--disable-background-networking",
                "--virtual-time-budget=4000",
            ])
            .arg(format!("--window-size={CAPTURE_SIZE}"))
            .arg(format!(
                "--user-data-dir={}",
                self.profile.to_string_lossy()
            ))
            .arg(format!("--screenshot={}", target.to_string_lossy()))
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let failed = |cause: String| {
            AppError::new(AppErrorCode::Unknown, CAPTURE_FAILED).with_detail("raw", cause)
        };
        let mut child = command.spawn().map_err(|error| failed(error.to_string()))?;
        match tokio::time::timeout(CAPTURE_TIMEOUT, child.wait()).await {
            Ok(Ok(_)) if target.is_file() => Ok(()),
            Ok(Ok(status)) => Err(failed(status.to_string())),
            Ok(Err(error)) => Err(failed(error.to_string())),
            Err(_) => {
                let _ = child.kill().await;
                Err(failed("timed out".to_owned()))
            }
        }
    }
}

pub fn kind_of(owner: &str, format: &str) -> GalleryKind {
    match format {
        "html" | "htm" if owner == DASHBOARDS => GalleryKind::Dashboard,
        "pdf" | "docx" | "doc" | "odt" | "rtf" | "md" | "pptx" | "ppt" | "odp" | "html" | "htm" => {
            GalleryKind::Document
        }
        "xlsx" | "xlsm" | "xls" | "ods" | "csv" => GalleryKind::Table,
        "png" | "jpg" | "jpeg" | "svg" => GalleryKind::Image,
        _ => GalleryKind::Other,
    }
}

fn format_of(name: &str) -> String {
    Path::new(name)
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

fn plain(part: &str) -> bool {
    !part.is_empty() && part != "." && part != ".." && !part.contains(['/', '\\'])
}

fn item(owner: &str, name: &str, path: &Path, live: bool) -> Option<GalleryItemDto> {
    let metadata = std::fs::metadata(extended(path))
        .ok()
        .filter(std::fs::Metadata::is_file)?;
    let format = format_of(name);
    let valid = plain(owner)
        && plain(name)
        && owner.chars().count() <= GALLERY_OWNER_MAX
        && name.chars().count() <= ARTIFACT_NAME_MAX
        && !format.is_empty()
        && format.chars().count() <= ARTIFACT_FORMAT_MAX
        && is_artifact(path);
    if !valid {
        return None;
    }
    let modified_at = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .and_then(|elapsed| i64::try_from(elapsed.as_millis()).ok())
        .unwrap_or(0);
    Some(GalleryItemDto {
        id: format!("{owner}/{name}"),
        name: name.to_owned(),
        kind: kind_of(owner, &format),
        format,
        size_bytes: metadata.len(),
        modified_at,
        owner: owner.to_owned(),
        live,
    })
}

fn media_type(format: &str) -> Option<(&'static str, u64)> {
    match format {
        "png" => Some(("image/png", IMAGE_BYTES_MAX)),
        "jpg" | "jpeg" => Some(("image/jpeg", IMAGE_BYTES_MAX)),
        "svg" => Some(("image/svg+xml", SVG_BYTES_MAX)),
        "pdf" => Some(("application/pdf", PDF_BYTES_MAX)),
        _ => None,
    }
}

fn encoded(media_type: &str, bytes: &[u8]) -> GalleryThumbnailDto {
    GalleryThumbnailDto {
        media_type: Some(media_type.to_owned()),
        data: Some(STANDARD.encode(bytes)),
    }
}

fn cache_key(path: &Path) -> String {
    Sha256::digest(path.to_string_lossy().as_bytes())
        .iter()
        .take(12)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

impl ArtifactStore {
    pub fn gallery(&self) -> GalleryListDto {
        let mut items = Vec::new();
        for (library, folder) in &self.libraries {
            let Ok(entries) = std::fs::read_dir(folder) else {
                continue;
            };
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                let live = library == DASHBOARDS
                    && self
                        .dashboards
                        .as_ref()
                        .is_some_and(|service| service.is_live(&name));
                items.extend(item(library, &name, &entry.path(), live));
            }
        }
        if let Ok(owners) = std::fs::read_dir(self.folders.root()) {
            for owner in owners.flatten() {
                if !owner.file_type().is_ok_and(|kind| kind.is_dir()) {
                    continue;
                }
                let owner = owner.file_name().to_string_lossy().into_owned();
                if self.library(&owner).is_some() {
                    continue;
                }
                for output in self.folders.outputs(&owner) {
                    items.extend(item(&owner, &output.name, &output.path, false));
                }
            }
        }
        items.sort_by(|a, b| {
            b.modified_at
                .cmp(&a.modified_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        items.dedup_by(|a, b| a.id == b.id);
        items.truncate(GALLERY_ITEMS_MAX);
        GalleryListDto { items }
    }

    pub async fn thumbnail(&self, id: &str) -> Result<GalleryThumbnailDto, AppError> {
        let path = self.resolve(id)?;
        let format = format_of(&path.to_string_lossy());
        if let Some((media_type, limit)) = media_type(&format) {
            let size = std::fs::metadata(extended(&path))
                .map(|metadata| metadata.len())
                .unwrap_or(u64::MAX);
            if size > limit {
                return Ok(GalleryThumbnailDto::default());
            }
            return Ok(std::fs::read(extended(&path))
                .map(|bytes| encoded(media_type, &bytes))
                .unwrap_or_default());
        }
        if !matches!(format.as_str(), "html" | "htm") {
            return Ok(GalleryThumbnailDto::default());
        }
        let Some((cache, capture)) = self.thumbnails.clone() else {
            return Ok(GalleryThumbnailDto::default());
        };
        let metadata = std::fs::metadata(extended(&path))
            .map_err(|_| AppError::new(AppErrorCode::NotFound, super::ARTIFACT_NOT_FOUND))?;
        let stamp = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |elapsed| elapsed.as_millis());
        let key = cache_key(&path);
        let target = cache.join(format!("{key}-{stamp}-{}.png", metadata.len()));
        if !target.is_file() {
            let _gate = self.capture_gate.lock().await;
            if !target.is_file() {
                self.forget_thumbnails(&path);
                let _ = std::fs::create_dir_all(&cache);
                let url = format!("{}?{PREVIEW_QUERY}", self.page_server().await?.url(id)?);
                let partial = target.with_extension("part.png");
                let captured = capture.capture(&url, &partial).await;
                if captured.is_err() || std::fs::rename(&partial, &target).is_err() {
                    let _ = std::fs::remove_file(&partial);
                    return Ok(GalleryThumbnailDto::default());
                }
            }
        }
        Ok(std::fs::read(&target)
            .map(|bytes| encoded("image/png", &bytes))
            .unwrap_or_default())
    }

    fn forget_thumbnails(&self, path: &Path) {
        let Some((cache, _)) = &self.thumbnails else {
            return;
        };
        let prefix = format!("{}-", cache_key(path));
        let Ok(entries) = std::fs::read_dir(cache) else {
            return;
        };
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().starts_with(&prefix) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    pub fn remove_artifact(&self, id: &str) -> Result<(), AppError> {
        let path = self.resolve(id)?;
        std::fs::remove_file(extended(&path)).map_err(|cause| {
            AppError::new(AppErrorCode::Unknown, REMOVE_FAILED)
                .with_detail("raw", cause.to_string())
        })?;
        self.forget_thumbnails(&path);
        if let Some((owner, name)) = id.split_once('/')
            && owner == DASHBOARDS
            && let Some(service) = &self.dashboards
        {
            let _ = service.forget(name);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
