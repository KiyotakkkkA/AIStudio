use serde::{Deserialize, Serialize};

use crate::artifacts::{ARTIFACT_PATH_MAX, check_artifact_id};
use crate::chat::ChatCitationDto;
use crate::enums::wire_enum;
use crate::errors::AppError;
use crate::ids::DocumentId;
use crate::primitives::{MAX_SAFE_INTEGER, Timestamp};
use crate::validate::{Validate, check, invalid, nested, no_refinements};

pub const DOCUMENT_PATH_MAX: usize = 32_767;
pub const DOCUMENT_NAME_MAX: usize = 1024;
pub const DOCUMENT_MIME_MAX: usize = 128;
pub const DOCUMENT_LANGUAGE_MAX: usize = 64;
pub const DOCUMENT_TEXT_PAGE_MAX: usize = 262_144;
pub const DOCUMENT_BYTES_PAGE_MAX: usize = 4_194_304;
pub const DOCUMENT_BYTES_DATA_MAX: usize = DOCUMENT_BYTES_PAGE_MAX.div_ceil(3) * 4;
pub const DOCUMENT_LIST_MAX: usize = 1000;

fn positive(field: &str, value: u64) -> Result<(), AppError> {
    check::int_range(field, value, 1, MAX_SAFE_INTEGER.unsigned_abs())
}

wire_enum!(DocumentKind {
    Pdf => "pdf",
    Image => "image",
    Markdown => "markdown",
    Code => "code",
    Csv => "csv",
    Text => "text",
    Office => "office",
});

wire_enum!(DocumentOfficeFormat {
    Docx => "docx",
    Xlsx => "xlsx",
    Pptx => "pptx",
});

impl DocumentKind {
    pub const fn is_text(self) -> bool {
        matches!(self, Self::Markdown | Self::Code | Self::Csv | Self::Text)
    }
}

wire_enum!(DocumentTextEncoding {
    Utf8 => "utf-8",
    Utf16Le => "utf-16le",
    Utf16Be => "utf-16be",
    Windows1251 => "windows-1251",
});

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "camelCase")]
pub enum DocumentRef {
    Path {
        path: String,
    },
    Citation {
        citation: ChatCitationDto,
    },
    #[serde(rename_all = "camelCase")]
    StoreDocument {
        document_id: DocumentId,
    },
    #[serde(rename_all = "camelCase")]
    Artifact {
        artifact_id: String,
    },
}

impl Validate for DocumentRef {
    fn validate(&self) -> Result<(), AppError> {
        match self {
            Self::Path { path } => {
                check::non_empty("path", path)?;
                check::max_len("path", path, DOCUMENT_PATH_MAX)
            }
            Self::Citation { citation } => {
                check::non_empty("citation.documentId", &citation.document_id)?;
                nested("citation", citation.validate())
            }
            Self::StoreDocument { .. } => Ok(()),
            Self::Artifact { artifact_id } => check_artifact_id("artifactId", artifact_id),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentDto {
    pub id: DocumentId,
    pub content_hash: String,
    pub name: String,
    pub source_path: String,
    pub kind: DocumentKind,
    pub mime_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub office_format: Option<DocumentOfficeFormat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    pub size_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_count: Option<u64>,
    pub part_count: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_page: Option<u64>,
    pub open: bool,
    pub opened_at: Timestamp,
}

impl Validate for DocumentDto {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("contentHash", &self.content_hash)?;
        check::max_len("contentHash", &self.content_hash, 128)?;
        check::non_empty("name", &self.name)?;
        check::max_len("name", &self.name, DOCUMENT_NAME_MAX)?;
        check::non_empty("sourcePath", &self.source_path)?;
        check::max_len("sourcePath", &self.source_path, DOCUMENT_PATH_MAX)?;
        check::non_empty("mimeType", &self.mime_type)?;
        check::max_len("mimeType", &self.mime_type, DOCUMENT_MIME_MAX)?;
        if self.office_format.is_some() != (self.kind == DocumentKind::Office) {
            return Err(invalid("officeFormat", "kind"));
        }
        if let Some(language) = &self.language {
            check::non_empty("language", language)?;
            check::max_len("language", language, DOCUMENT_LANGUAGE_MAX)?;
        }
        check::safe_integer("sizeBytes", self.size_bytes)?;
        if let Some(pages) = self.page_count {
            positive("pageCount", pages)?;
        }
        positive("partCount", self.part_count)?;
        if let Some(page) = self.initial_page {
            positive("initialPage", page)?;
        }
        check::timestamp("openedAt", self.opened_at)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentHandle {
    pub id: DocumentId,
}

no_refinements!(DocumentHandle);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentPageInput {
    pub id: DocumentId,
    pub index: u64,
}

impl Validate for DocumentPageInput {
    fn validate(&self) -> Result<(), AppError> {
        check::safe_integer("index", self.index)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "content", rename_all = "camelCase")]
pub enum DocumentPageDto {
    #[serde(rename_all = "camelCase")]
    Bytes {
        index: u64,
        part_count: u64,
        offset: u64,
        total_bytes: u64,
        data: String,
    },
    #[serde(rename_all = "camelCase")]
    Text {
        index: u64,
        part_count: u64,
        first_line: u64,
        encoding: DocumentTextEncoding,
        text: String,
        untrusted: bool,
    },
}

fn check_part(index: u64, part_count: u64) -> Result<(), AppError> {
    positive("partCount", part_count)?;
    if index >= part_count {
        return Err(invalid("index", "max"));
    }
    Ok(())
}

impl Validate for DocumentPageDto {
    fn validate(&self) -> Result<(), AppError> {
        match self {
            Self::Bytes {
                index,
                part_count,
                offset,
                total_bytes,
                data,
            } => {
                check_part(*index, *part_count)?;
                check::safe_integer("offset", *offset)?;
                check::safe_integer("totalBytes", *total_bytes)?;
                if offset > total_bytes {
                    return Err(invalid("offset", "max"));
                }
                check::max_len("data", data, DOCUMENT_BYTES_DATA_MAX)
            }
            Self::Text {
                index,
                part_count,
                first_line,
                text,
                untrusted,
                ..
            } => {
                check_part(*index, *part_count)?;
                positive("firstLine", *first_line)?;
                check::max_len("text", text, DOCUMENT_TEXT_PAGE_MAX)?;
                check::literal_true("untrusted", *untrusted)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSaveResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

impl Validate for DocumentSaveResult {
    fn validate(&self) -> Result<(), AppError> {
        if let Some(path) = &self.path {
            check::non_empty("path", path)?;
            check::max_len("path", path, ARTIFACT_PATH_MAX)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate::parse;
    use serde_json::json;

    const DOCUMENT: &str = "0199aa11-1111-7111-8111-111111111111";

    #[test]
    fn refs_are_tagged_by_source() {
        let store =
            parse::<DocumentRef>(json!({ "source": "storeDocument", "documentId": DOCUMENT }))
                .expect("a store document ref");
        assert_eq!(
            store,
            DocumentRef::StoreDocument {
                document_id: DocumentId::parse(DOCUMENT).expect("uuid"),
            }
        );
        assert!(parse::<DocumentRef>(json!({ "source": "path", "path": "" })).is_err());
        assert!(parse::<DocumentRef>(json!({ "source": "url", "url": "x" })).is_err());
    }

    #[test]
    fn text_pages_are_always_untrusted() {
        let page = json!({
            "content": "text",
            "index": 0,
            "partCount": 1,
            "firstLine": 1,
            "encoding": "utf-8",
            "text": "# Plan",
            "untrusted": false,
        });
        assert!(parse::<DocumentPageDto>(page).is_err());
    }

    #[test]
    fn a_page_index_stays_inside_the_document() {
        let page = json!({
            "content": "bytes",
            "index": 2,
            "partCount": 2,
            "offset": 0,
            "totalBytes": 4,
            "data": "JVBERg==",
        });
        assert!(parse::<DocumentPageDto>(page).is_err());
    }
}
