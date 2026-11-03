use serde::{Deserialize, Serialize};

use crate::{
    AppError,
    validate::{Validate, check, invalid, nested},
};

pub const IMAGES_MAX: usize = 5;
pub const IMAGE_BYTES_MAX: u64 = 20 * 1024 * 1024;
/// Base64 inflates the 20 MiB byte cap by a third; the data URL prefix adds a few bytes.
pub const IMAGE_DATA_URL_MAX: usize = 30 * 1024 * 1024;

/// A host-owned image reference. No local path, pixels, credentials or vendor URL crosses IPC.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageAttachmentDto {
    pub id: String,
    pub name: String,
    pub mime_type: String,
    pub size_bytes: u64,
}

impl Validate for ImageAttachmentDto {
    fn validate(&self) -> Result<(), AppError> {
        if crate::ids::wire_uuid::parse(&self.id).is_none() {
            return Err(invalid("id", "uuid"));
        }
        check::trimmed_name("name", &self.name, 255)?;
        if !matches!(
            self.mime_type.as_str(),
            "image/png" | "image/jpeg" | "image/gif" | "image/webp"
        ) {
            return Err(invalid("mimeType", "enum"));
        }
        check::int_range("sizeBytes", self.size_bytes, 1, IMAGE_BYTES_MAX)
    }
}

/// Reads one stored copy for an in-app preview. Ownership and integrity are checked host-side.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageReadInput {
    pub id: crate::ConversationId,
    pub image: ImageAttachmentDto,
}

impl Validate for ImageReadInput {
    fn validate(&self) -> Result<(), AppError> {
        nested("image", self.image.validate())
    }
}

/// A stored image as a data URL; only the renderer that owns the conversation receives pixels.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageDataDto {
    pub data_url: String,
}

impl Validate for ImageDataDto {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("dataUrl", &self.data_url)?;
        check::max_len("dataUrl", &self.data_url, IMAGE_DATA_URL_MAX)
    }
}

pub fn validate_images(images: &[ImageAttachmentDto]) -> Result<(), AppError> {
    if images.len() > IMAGES_MAX {
        return Err(invalid("images", "maxItems"));
    }
    for (index, image) in images.iter().enumerate() {
        nested(&format!("images.{index}"), image.validate())?;
        if images[..index].iter().any(|other| other.id == image.id) {
            return Err(invalid("images", "unique"));
        }
    }
    Ok(())
}
