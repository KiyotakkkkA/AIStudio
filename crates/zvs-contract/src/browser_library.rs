use serde::{Deserialize, Serialize};

use crate::browser::URL_MAX;
use crate::enums::wire_enum;
use crate::errors::AppError;
use crate::ids::is_wire_uuid;
use crate::primitives::Timestamp;
use crate::validate::{Validate, check, invalid, nested};

pub const HISTORY_TITLE_MAX: usize = 2048;
pub const HISTORY_QUERY_MAX: usize = 512;
pub const HISTORY_PAGE_MAX: usize = 500;
pub const BOOKMARKS_MAX: usize = 5000;
pub const BOOKMARK_FOLDER_MAX: usize = 1024;
pub const IMPORT_SOURCES_MAX: usize = 64;
pub const IMPORT_LABEL_MAX: usize = 256;
pub const IMPORT_SOURCE_ID_MAX: usize = 4096;

wire_enum!(BrowserImportKind {
    Chrome => "chrome",
    Edge => "edge",
    Brave => "brave",
    Vivaldi => "vivaldi",
    Yandex => "yandex",
    Opera => "opera",
    Firefox => "firefox",
});

fn uuid(field: &str, id: &str) -> Result<(), AppError> {
    if is_wire_uuid(id) {
        Ok(())
    } else {
        Err(invalid(field, "uuid"))
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserHistoryEntryDto {
    pub id: String,
    pub url: String,
    pub title: String,
    pub visited_at: Timestamp,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub imported_from: Option<BrowserImportKind>,
}

impl Validate for BrowserHistoryEntryDto {
    fn validate(&self) -> Result<(), AppError> {
        uuid("id", &self.id)?;
        check::url("url", &self.url, URL_MAX)?;
        check::max_len("title", &self.title, HISTORY_TITLE_MAX)?;
        check::timestamp("visitedAt", self.visited_at)
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserHistoryDto {
    pub items: Vec<BrowserHistoryEntryDto>,
    pub total: u64,
}

impl Validate for BrowserHistoryDto {
    fn validate(&self) -> Result<(), AppError> {
        if self.items.len() > HISTORY_PAGE_MAX {
            return Err(invalid("items", "max"));
        }
        check::safe_integer("total", self.total)?;
        nested("items", self.items.validate())
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserHistoryQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
}

impl Validate for BrowserHistoryQuery {
    fn validate(&self) -> Result<(), AppError> {
        self.query.as_deref().map_or(Ok(()), |query| {
            check::max_len("query", query, HISTORY_QUERY_MAX)
        })
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserLibraryRef {
    pub id: String,
}

impl Validate for BrowserLibraryRef {
    fn validate(&self) -> Result<(), AppError> {
        uuid("id", &self.id)
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserBookmarkDto {
    pub id: String,
    pub url: String,
    pub title: String,
    pub folder: String,
    pub created_at: Timestamp,
}

impl Validate for BrowserBookmarkDto {
    fn validate(&self) -> Result<(), AppError> {
        uuid("id", &self.id)?;
        check::url("url", &self.url, URL_MAX)?;
        check::max_len("title", &self.title, HISTORY_TITLE_MAX)?;
        check::max_len("folder", &self.folder, BOOKMARK_FOLDER_MAX)?;
        check::timestamp("createdAt", self.created_at)
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserBookmarksDto {
    pub items: Vec<BrowserBookmarkDto>,
}

impl Validate for BrowserBookmarksDto {
    fn validate(&self) -> Result<(), AppError> {
        if self.items.len() > BOOKMARKS_MAX {
            return Err(invalid("items", "max"));
        }
        nested("items", self.items.validate())
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserBookmarkRenameInput {
    pub id: String,
    pub title: String,
}

impl Validate for BrowserBookmarkRenameInput {
    fn validate(&self) -> Result<(), AppError> {
        uuid("id", &self.id)?;
        check::non_empty("title", &self.title)?;
        check::max_len("title", &self.title, HISTORY_TITLE_MAX)
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserImportSourceDto {
    pub id: String,
    pub kind: BrowserImportKind,
    pub profile: String,
    pub history: bool,
    pub bookmarks: bool,
}

impl Validate for BrowserImportSourceDto {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("id", &self.id)?;
        check::max_len("id", &self.id, IMPORT_SOURCE_ID_MAX)?;
        check::max_len("profile", &self.profile, IMPORT_LABEL_MAX)
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserImportSourcesDto {
    pub sources: Vec<BrowserImportSourceDto>,
}

impl Validate for BrowserImportSourcesDto {
    fn validate(&self) -> Result<(), AppError> {
        if self.sources.len() > IMPORT_SOURCES_MAX {
            return Err(invalid("sources", "max"));
        }
        nested("sources", self.sources.validate())
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserImportInput {
    pub source: String,
    pub history: bool,
    pub bookmarks: bool,
}

impl Validate for BrowserImportInput {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("source", &self.source)?;
        check::max_len("source", &self.source, IMPORT_SOURCE_ID_MAX)?;
        if self.history || self.bookmarks {
            Ok(())
        } else {
            Err(invalid("history", "oneOf"))
        }
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserImportResultDto {
    pub history: u64,
    pub bookmarks: u64,
}

impl Validate for BrowserImportResultDto {
    fn validate(&self) -> Result<(), AppError> {
        check::safe_integer("history", self.history)?;
        check::safe_integer("bookmarks", self.bookmarks)
    }
}
