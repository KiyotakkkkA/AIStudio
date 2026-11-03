//! Reusable image loading stage: import picked files, persist immutable references, resolve pixels.
use std::fmt;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;
use zvs_contract::images::{IMAGE_BYTES_MAX, IMAGES_MAX};
use zvs_contract::{AppError, AppErrorCode, ImageAttachmentDto, Validate};

use crate::artifacts::ArtifactStore;
use crate::drivers::ai::ChatMessage;
use crate::platform::long_path::extended;

pub const IMAGE_INVALID: &str = "Выберите изображение PNG, JPEG, GIF или WebP размером до 20 МБ";
pub const IMAGE_MISSING: &str =
    "Не удалось загрузить прикреплённое изображение. Прикрепите его повторно.";
pub const IMAGES_FULL: &str = "К сообщению можно прикрепить не больше 5 изображений";
pub const VISION_UNSUPPORTED: &str = "Изображения доступны только для провайдеров Qwen и Z.ai";
pub const UPLOAD_FAILED: &str = "Не удалось загрузить изображение провайдеру. Повторите запрос.";
pub const IMAGE_TOKEN_ESTIMATE: i64 = 4096;
const LEGACY_DIRECTORY: &str = "_images";

#[derive(Clone, PartialEq, Eq)]
pub struct ImageInput {
    pub attachment: ImageAttachmentDto,
    pub bytes: Arc<[u8]>,
}

impl ImageInput {
    pub fn reference(attachment: ImageAttachmentDto) -> Self {
        Self {
            attachment,
            bytes: Arc::from([]),
        }
    }
}

impl fmt::Debug for ImageInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImageInput")
            .field("id", &self.attachment.id)
            .field("bytes", &self.bytes.len())
            .finish()
    }
}

#[derive(Serialize, Deserialize)]
struct Manifest {
    attachment: ImageAttachmentDto,
    hash: String,
}

#[derive(Clone)]
pub struct ImageLoader {
    store: ArtifactStore,
}

fn failure(message: &str) -> AppError {
    AppError::new(AppErrorCode::ValidationFailed, message)
}

pub fn validate_inputs(messages: &[ChatMessage]) -> Result<(), AppError> {
    let mut count = 0;
    for message in messages {
        if !message.images.is_empty() && message.role != crate::drivers::ai::ChatRole::User {
            return Err(failure(IMAGE_INVALID));
        }
        for image in &message.images {
            count += 1;
            image.attachment.validate()?;
            if image.bytes.len() as u64 != image.attachment.size_bytes
                || mime(&image.bytes) != Some(image.attachment.mime_type.as_str())
            {
                return Err(failure(IMAGE_INVALID));
            }
        }
    }
    if count > IMAGES_MAX {
        return Err(failure(IMAGES_FULL));
    }
    Ok(())
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") && bytes.len() >= 33 {
        Some("image/png")
    } else if bytes.starts_with(b"\xff\xd8\xff") && bytes.len() >= 4 {
        Some("image/jpeg")
    } else if (bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")) && bytes.len() >= 13 {
        Some("image/gif")
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") && bytes.len() >= 20 {
        Some("image/webp")
    } else {
        None
    }
}

/// Moves copies stored before the attachments folder existed next to the conversation root.
fn migrate_legacy(owner_folder: &Path) -> Result<(), AppError> {
    let legacy = owner_folder
        .join(crate::conversations::WORK_DIR)
        .join(LEGACY_DIRECTORY);
    let target = owner_folder.join(crate::conversations::ATTACHMENTS_DIR);
    if extended(&target).exists() || !extended(&legacy).is_dir() {
        return Ok(());
    }
    std::fs::rename(extended(&legacy), extended(&target)).map_err(|_| failure(IMAGE_MISSING))
}

fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, AppError> {
    let file = std::fs::File::open(extended(path)).map_err(|_| failure(IMAGE_MISSING))?;
    if !file
        .metadata()
        .is_ok_and(|m| m.is_file() && m.len() <= limit)
    {
        return Err(failure(IMAGE_INVALID));
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| failure(IMAGE_MISSING))?;
    if bytes.is_empty() || bytes.len() as u64 > limit {
        return Err(failure(IMAGE_INVALID));
    }
    Ok(bytes)
}

impl ImageLoader {
    pub const fn new(store: ArtifactStore) -> Self {
        Self { store }
    }

    fn folder(&self, owner: &str) -> Result<PathBuf, AppError> {
        if !crate::conversations::is_owner(owner) {
            return Err(failure(IMAGE_MISSING));
        }
        std::fs::create_dir_all(extended(self.store.root())).map_err(|_| failure(IMAGE_MISSING))?;
        let mut folder = std::fs::canonicalize(extended(self.store.root()))
            .map_err(|_| failure(IMAGE_MISSING))?;
        for name in [owner, crate::conversations::ATTACHMENTS_DIR] {
            if name == crate::conversations::ATTACHMENTS_DIR {
                migrate_legacy(&folder)?;
            }
            let next = folder.join(name);
            match std::fs::create_dir(extended(&next)) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(_) => return Err(failure(IMAGE_MISSING)),
            }
            let actual =
                std::fs::canonicalize(extended(&next)).map_err(|_| failure(IMAGE_MISSING))?;
            if actual != next {
                return Err(failure(IMAGE_MISSING));
            }
            folder = actual;
        }
        Ok(folder)
    }

    /// Accepts paths selected by the person through the host picker or the same-user CLI.
    /// Never expose this loader as a model tool or to an untrusted webview.
    pub fn import(
        &self,
        owner: &str,
        paths: &[String],
    ) -> Result<Vec<ImageAttachmentDto>, AppError> {
        if paths.len() > IMAGES_MAX {
            return Err(failure(IMAGES_FULL));
        }
        let folder = self.folder(owner)?;
        let mut prepared = Vec::new();
        for path in paths {
            let bytes = read_bounded(Path::new(path), IMAGE_BYTES_MAX)?;
            let mime_type = mime(&bytes).ok_or_else(|| failure(IMAGE_INVALID))?;
            let name = Path::new(path)
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| failure(IMAGE_INVALID))?;
            let attachment = ImageAttachmentDto {
                id: uuid::Uuid::new_v4().to_string(),
                name: zvs_contract::validate::js_trim(name).to_owned(),
                mime_type: mime_type.to_owned(),
                size_bytes: bytes.len() as u64,
            };
            attachment.validate()?;
            prepared.push((attachment, bytes));
        }
        let changes: Vec<_> = prepared
            .iter()
            .flat_map(|(image, bytes)| {
                [
                    (folder.join(&image.id), Some(bytes.len() as u64)),
                    (folder.join(format!("{}.json", image.id)), Some(1024)),
                ]
            })
            .collect();
        crate::chat_files::within_quota(
            folder.parent().ok_or_else(|| failure(IMAGE_MISSING))?,
            &changes,
        )?;
        let mut written = Vec::new();
        let result = (|| {
            for (image, bytes) in &prepared {
                let manifest = Manifest {
                    attachment: image.clone(),
                    hash: digest(bytes),
                };
                for (path, data) in [
                    (folder.join(&image.id), bytes.clone()),
                    (
                        folder.join(format!("{}.json", image.id)),
                        serde_json::to_vec(&manifest).map_err(|_| failure(IMAGE_MISSING))?,
                    ),
                ] {
                    written.push(path.clone());
                    std::fs::write(extended(&path), data).map_err(|_| failure(IMAGE_MISSING))?;
                }
            }
            Ok(prepared.into_iter().map(|(image, _)| image).collect())
        })();
        if result.is_err() {
            for path in written {
                let _ = std::fs::remove_file(extended(&path));
            }
        }
        result
    }

    pub fn validate_reference(
        &self,
        owner: &str,
        image: &ImageAttachmentDto,
    ) -> Result<(), AppError> {
        image.validate()?;
        let folder = self.folder(owner)?;
        let path = std::fs::canonicalize(extended(&folder.join(format!("{}.json", image.id))))
            .map_err(|_| failure(IMAGE_MISSING))?;
        if path.parent() != Some(folder.as_path()) {
            return Err(failure(IMAGE_MISSING));
        }
        let manifest: Manifest = serde_json::from_slice(&read_bounded(&path, 4096)?)
            .map_err(|_| failure(IMAGE_MISSING))?;
        if manifest.attachment != *image {
            return Err(failure(IMAGE_MISSING));
        }
        Ok(())
    }

    pub fn load(&self, owner: &str, image: &ImageAttachmentDto) -> Result<ImageInput, AppError> {
        image.validate()?;
        let folder = self.folder(owner)?;
        let safe = |name: &str| -> Result<PathBuf, AppError> {
            let path = std::fs::canonicalize(extended(&folder.join(name)))
                .map_err(|_| failure(IMAGE_MISSING))?;
            if path.parent() != Some(folder.as_path()) {
                return Err(failure(IMAGE_MISSING));
            }
            Ok(path)
        };
        let metadata = read_bounded(&safe(&format!("{}.json", image.id))?, 4096)?;
        let manifest: Manifest =
            serde_json::from_slice(&metadata).map_err(|_| failure(IMAGE_MISSING))?;
        let bytes = read_bounded(&safe(&image.id)?, IMAGE_BYTES_MAX)?;
        if manifest.attachment != *image
            || bytes.len() as u64 != image.size_bytes
            || mime(&bytes) != Some(image.mime_type.as_str())
            || manifest.hash != digest(&bytes)
        {
            return Err(failure(IMAGE_MISSING));
        }
        Ok(ImageInput {
            attachment: image.clone(),
            bytes: Arc::from(bytes),
        })
    }

    /// Resolve only the images retained by context windowing; callers reuse this before generation.
    pub async fn load_messages(
        &self,
        owner: &str,
        messages: &mut [ChatMessage],
        cancel: &CancellationToken,
    ) -> Result<(), AppError> {
        for message in messages {
            for image in &mut message.images {
                if cancel.is_cancelled() {
                    return Err(crate::drivers::ai::errors::cancelled());
                }
                let loader = self.clone();
                let owner = owner.to_owned();
                let attachment = image.attachment.clone();
                let task = tokio::task::spawn_blocking(move || loader.load(&owner, &attachment));
                *image = tokio::select! {
                    biased;
                    () = cancel.cancelled() => return Err(crate::drivers::ai::errors::cancelled()),
                    loaded = task => loaded.map_err(|_| failure(IMAGE_MISSING))??,
                };
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
