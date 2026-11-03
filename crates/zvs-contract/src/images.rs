use serde::{Deserialize, Serialize};

use crate::{
    AppError,
    validate::{Validate, check, invalid, nested},
};

pub const IMAGES_MAX: usize = 5;
pub const IMAGE_BYTES_MAX: u64 = 20 * 1024 * 1024;

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
