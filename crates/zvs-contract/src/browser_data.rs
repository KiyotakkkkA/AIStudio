use serde::{Deserialize, Serialize};

use crate::browser::URL_MAX;
use crate::enums::wire_enum;
use crate::errors::AppError;
use crate::ids::is_wire_uuid;
use crate::primitives::Timestamp;
use crate::validate::{Validate, check, invalid, nested};

pub const DOWNLOAD_NAME_MAX: usize = 1024;
pub const DOWNLOAD_PATH_MAX: usize = 32_767;
pub const DOWNLOAD_HISTORY_MAX: usize = 500;

wire_enum!(BrowserDownloadState {
    Done => "done",
    Failed => "failed",
    Cancelled => "cancelled",
});

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserDownloadDto {
    pub id: String,
    pub url: String,
    pub file_name: String,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub path: Option<String>,
    pub bytes: u64,
    pub state: BrowserDownloadState,
    pub agent: bool,
    pub finished_at: Timestamp,
}

impl Validate for BrowserDownloadDto {
    fn validate(&self) -> Result<(), AppError> {
        if !is_wire_uuid(&self.id) {
            return Err(invalid("id", "uuid"));
        }
        check::url("url", &self.url, URL_MAX)?;
        check::non_empty("fileName", &self.file_name)?;
        check::max_len("fileName", &self.file_name, DOWNLOAD_NAME_MAX)?;
        if let Some(path) = &self.path {
            check::non_empty("path", path)?;
            check::max_len("path", path, DOWNLOAD_PATH_MAX)?;
        }
        check::safe_integer("bytes", self.bytes)?;
        check::timestamp("finishedAt", self.finished_at)
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserDownloadRef {
    pub id: String,
}

impl Validate for BrowserDownloadRef {
    fn validate(&self) -> Result<(), AppError> {
        if is_wire_uuid(&self.id) {
            Ok(())
        } else {
            Err(invalid("id", "uuid"))
        }
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserDownloadsDto {
    pub items: Vec<BrowserDownloadDto>,
}

impl Validate for BrowserDownloadsDto {
    fn validate(&self) -> Result<(), AppError> {
        if self.items.len() > DOWNLOAD_HISTORY_MAX {
            return Err(invalid("items", "max"));
        }
        nested("items", self.items.validate())
    }
}
