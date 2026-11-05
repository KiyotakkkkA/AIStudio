use serde::{Deserialize, Serialize};

use crate::enums::wire_enum;
use crate::errors::AppError;
use crate::validate::{Validate, check, invalid};

pub const ARTIFACT_ID_MAX: usize = 512;
pub const ARTIFACT_NAME_MAX: usize = 255;
pub const ARTIFACT_FORMAT_MAX: usize = 16;
pub const ARTIFACT_PATH_MAX: usize = 32_767;
pub const ARTIFACT_APPLICATION_MAX: usize = 256;
pub const BROWSER_ID_MAX: usize = 256;
pub const BROWSERS_MAX: usize = 32;

fn check_id(id: &str) -> Result<(), AppError> {
    check_artifact_id("id", id)
}

pub fn check_artifact_id(field: &str, id: &str) -> Result<(), AppError> {
    check::non_empty(field, id)?;
    check::max_len(field, id, ARTIFACT_ID_MAX)?;
    let mut parts = id.split('/');
    let valid = matches!(
        (parts.next(), parts.next(), parts.next()),
        (Some(run), Some(name), None)
            if !run.is_empty()
                && !name.is_empty()
                && ![run, name].iter().any(|part| *part == "." || *part == ".." || part.contains('\\'))
    );
    if valid {
        Ok(())
    } else {
        Err(invalid(field, "format"))
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactDto {
    pub id: String,
    pub name: String,
    pub format: String,
    pub size_bytes: u64,
}

impl Validate for ArtifactDto {
    fn validate(&self) -> Result<(), AppError> {
        check_id(&self.id)?;
        check::non_empty("name", &self.name)?;
        check::max_len("name", &self.name, ARTIFACT_NAME_MAX)?;
        check::non_empty("format", &self.format)?;
        check::max_len("format", &self.format, ARTIFACT_FORMAT_MAX)?;
        check::safe_integer("sizeBytes", self.size_bytes)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactRef {
    pub id: String,
}

impl Validate for ArtifactRef {
    fn validate(&self) -> Result<(), AppError> {
        check_id(&self.id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactSaveResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

impl Validate for ArtifactSaveResult {
    fn validate(&self) -> Result<(), AppError> {
        if let Some(path) = &self.path {
            check::non_empty("path", path)?;
            check::max_len("path", path, ARTIFACT_PATH_MAX)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactApplicationDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl Validate for ArtifactApplicationDto {
    fn validate(&self) -> Result<(), AppError> {
        if let Some(name) = &self.name {
            check::non_empty("name", name)?;
            check::max_len("name", name, ARTIFACT_APPLICATION_MAX)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledBrowserDto {
    pub id: String,
    pub name: String,
}

impl Validate for InstalledBrowserDto {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("id", &self.id)?;
        check::max_len("id", &self.id, BROWSER_ID_MAX)?;
        check::non_empty("name", &self.name)?;
        check::max_len("name", &self.name, ARTIFACT_APPLICATION_MAX)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactBrowsersDto {
    pub browsers: Vec<InstalledBrowserDto>,
}

impl Validate for ArtifactBrowsersDto {
    fn validate(&self) -> Result<(), AppError> {
        if self.browsers.len() > BROWSERS_MAX {
            return Err(invalid("browsers", "maxLength"));
        }
        crate::validate::nested("browsers", self.browsers.validate())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenArtifactInBrowserInput {
    pub id: String,
    pub browser: String,
}

impl Validate for OpenArtifactInBrowserInput {
    fn validate(&self) -> Result<(), AppError> {
        check_id(&self.id)?;
        check::non_empty("browser", &self.browser)?;
        check::max_len("browser", &self.browser, BROWSER_ID_MAX)
    }
}

pub const DASHBOARD_INTERVAL_MIN: u32 = 15;
pub const DASHBOARD_INTERVAL_MAX: u32 = 86_400;
pub const DASHBOARD_SCRIPTS_MAX: usize = 16;
pub const DASHBOARD_SCRIPT_SOURCE_MAX: usize = 65_536;
pub const DASHBOARD_ERROR_MAX: usize = 2_000;
pub const SHA256_HEX_LEN: usize = 64;

fn check_interval(field: &str, interval: u32) -> Result<(), AppError> {
    if interval == 0 || (DASHBOARD_INTERVAL_MIN..=DASHBOARD_INTERVAL_MAX).contains(&interval) {
        Ok(())
    } else {
        Err(invalid(field, "range"))
    }
}

fn check_sha256(field: &str, value: &str) -> Result<(), AppError> {
    if value.len() == SHA256_HEX_LEN
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(invalid(field, "format"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardScriptDto {
    pub name: String,
    pub sha256: String,
    pub approved: bool,
    pub source: String,
    pub truncated: bool,
}

impl Validate for DashboardScriptDto {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("name", &self.name)?;
        check::max_len("name", &self.name, ARTIFACT_NAME_MAX)?;
        check_sha256("sha256", &self.sha256)?;
        check::max_len("source", &self.source, DASHBOARD_SCRIPT_SOURCE_MAX)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardStatusDto {
    pub id: String,
    pub live: bool,
    pub interval: u32,
    pub spec_interval: u32,
    pub overridden: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refreshed_at: Option<crate::primitives::Timestamp>,
    pub refreshing: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub scripts: Vec<DashboardScriptDto>,
}

impl Validate for DashboardStatusDto {
    fn validate(&self) -> Result<(), AppError> {
        check_id(&self.id)?;
        check_interval("interval", self.interval)?;
        check_interval("specInterval", self.spec_interval)?;
        if let Some(refreshed_at) = self.refreshed_at
            && refreshed_at < 0
        {
            return Err(invalid("refreshedAt", "min"));
        }
        if let Some(error) = &self.error {
            check::non_empty("error", error)?;
            check::max_len("error", error, DASHBOARD_ERROR_MAX)?;
        }
        if self.scripts.len() > DASHBOARD_SCRIPTS_MAX {
            return Err(invalid("scripts", "maxLength"));
        }
        crate::validate::nested("scripts", self.scripts.validate())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardApproveInput {
    pub id: String,
    pub hashes: Vec<String>,
}

impl Validate for DashboardApproveInput {
    fn validate(&self) -> Result<(), AppError> {
        check_id(&self.id)?;
        if self.hashes.is_empty() || self.hashes.len() > DASHBOARD_SCRIPTS_MAX {
            return Err(invalid("hashes", "length"));
        }
        self.hashes
            .iter()
            .try_for_each(|hash| check_sha256("hashes", hash))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardIntervalInput {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interval: Option<u32>,
}

impl Validate for DashboardIntervalInput {
    fn validate(&self) -> Result<(), AppError> {
        check_id(&self.id)?;
        self.interval
            .map_or(Ok(()), |interval| check_interval("interval", interval))
    }
}

pub const GALLERY_ITEMS_MAX: usize = 4_096;
pub const GALLERY_OWNER_MAX: usize = 256;
pub const GALLERY_THUMBNAIL_BYTES_MAX: usize = 16 * 1024 * 1024;
pub const GALLERY_THUMBNAIL_DATA_MAX: usize = GALLERY_THUMBNAIL_BYTES_MAX.div_ceil(3) * 4;
pub const GALLERY_MEDIA_TYPES: &[&str] = &[
    "image/png",
    "image/jpeg",
    "image/svg+xml",
    "application/pdf",
];

wire_enum!(GalleryKind {
    Dashboard => "dashboard",
    Document => "document",
    Table => "table",
    Image => "image",
    Other => "other",
});

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryItemDto {
    pub id: String,
    pub name: String,
    pub format: String,
    pub kind: GalleryKind,
    pub size_bytes: u64,
    pub modified_at: crate::primitives::Timestamp,
    pub owner: String,
    pub live: bool,
}

impl Validate for GalleryItemDto {
    fn validate(&self) -> Result<(), AppError> {
        check_id(&self.id)?;
        check::non_empty("name", &self.name)?;
        check::max_len("name", &self.name, ARTIFACT_NAME_MAX)?;
        check::non_empty("format", &self.format)?;
        check::max_len("format", &self.format, ARTIFACT_FORMAT_MAX)?;
        check::safe_integer("sizeBytes", self.size_bytes)?;
        if self.modified_at < 0 {
            return Err(invalid("modifiedAt", "min"));
        }
        check::non_empty("owner", &self.owner)?;
        check::max_len("owner", &self.owner, GALLERY_OWNER_MAX)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryListDto {
    pub items: Vec<GalleryItemDto>,
}

impl Validate for GalleryListDto {
    fn validate(&self) -> Result<(), AppError> {
        if self.items.len() > GALLERY_ITEMS_MAX {
            return Err(invalid("items", "maxLength"));
        }
        crate::validate::nested("items", self.items.validate())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryThumbnailDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
}

impl Validate for GalleryThumbnailDto {
    fn validate(&self) -> Result<(), AppError> {
        match (&self.media_type, &self.data) {
            (None, None) => Ok(()),
            (Some(media_type), Some(data)) => {
                if !GALLERY_MEDIA_TYPES.contains(&media_type.as_str()) {
                    return Err(invalid("mediaType", "enum"));
                }
                check::non_empty("data", data)?;
                check::max_len("data", data, GALLERY_THUMBNAIL_DATA_MAX)
            }
            _ => Err(invalid("data", "pair")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate::parse;
    use serde_json::json;

    #[test]
    fn a_thumbnail_carries_both_its_type_and_data_or_neither() {
        assert!(parse::<GalleryThumbnailDto>(json!({})).is_ok());
        assert!(
            parse::<GalleryThumbnailDto>(json!({ "mediaType": "image/png", "data": "iVBO" }))
                .is_ok()
        );
        for value in [
            json!({ "mediaType": "image/png" }),
            json!({ "data": "iVBO" }),
            json!({ "mediaType": "text/html", "data": "PGgx" }),
        ] {
            assert!(
                parse::<GalleryThumbnailDto>(value.clone()).is_err(),
                "{value}"
            );
        }
    }

    #[test]
    fn a_refresh_interval_is_off_or_between_the_bounds() {
        let input = |interval| json!({ "id": "dashboards/a.html", "interval": interval });
        for valid in [json!(null), json!(0), json!(15), json!(86_400)] {
            assert!(
                parse::<DashboardIntervalInput>(input(valid.clone())).is_ok(),
                "{valid}"
            );
        }
        for invalid in [json!(1), json!(14), json!(86_401)] {
            assert!(
                parse::<DashboardIntervalInput>(input(invalid.clone())).is_err(),
                "{invalid}"
            );
        }
    }

    #[test]
    fn an_approval_names_lowercase_sha256_hashes() {
        let hash = "a".repeat(64);
        assert!(
            parse::<DashboardApproveInput>(json!({ "id": "dashboards/a.html", "hashes": [hash] }))
                .is_ok()
        );
        for hashes in [json!([]), json!(["A".repeat(64)]), json!(["abc"])] {
            assert!(
                parse::<DashboardApproveInput>(
                    json!({ "id": "dashboards/a.html", "hashes": hashes })
                )
                .is_err()
            );
        }
    }

    #[test]
    fn an_artifact_id_is_a_run_folder_and_a_file_name() {
        assert!(parse::<ArtifactRef>(json!({ "id": "run-1/report.docx" })).is_ok());
        for id in [
            "report.docx",
            "run/../x",
            "../x",
            "a/b/c",
            "run/",
            "run\\x/y",
            "/x",
        ] {
            assert!(parse::<ArtifactRef>(json!({ "id": id })).is_err(), "{id}");
        }
    }
}
