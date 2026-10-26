use serde::{Deserialize, Serialize};

use crate::errors::AppError;
use crate::validate::{Validate, check, invalid, js_trim};

pub const SITE_URL_MAX: usize = 4096;
pub const SITE_ICON_BYTES_MAX: usize = 65_536;
pub const SITE_ICON_DATA_MAX: usize = SITE_ICON_BYTES_MAX.div_ceil(3) * 4;
pub const SITE_ICON_MIMES: [&str; 5] = [
    "image/png",
    "image/x-icon",
    "image/jpeg",
    "image/gif",
    "image/webp",
];

pub fn check_web_url(field: &str, value: &str) -> Result<(), AppError> {
    check::url(field, value, SITE_URL_MAX)?;
    let scheme = url::Url::parse(js_trim(value))
        .map(|url| url.scheme().to_owned())
        .unwrap_or_default();
    if scheme == "https" || scheme == "http" {
        Ok(())
    } else {
        Err(invalid(field, "url"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenSiteInput {
    pub url: String,
}

impl Validate for OpenSiteInput {
    fn validate(&self) -> Result<(), AppError> {
        check_web_url("url", &self.url)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteIconInput {
    pub url: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub icon: Option<String>,
}

impl Validate for SiteIconInput {
    fn validate(&self) -> Result<(), AppError> {
        check_web_url("url", &self.url)?;
        self.icon
            .as_deref()
            .map_or(Ok(()), |icon| check_web_url("icon", icon))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteIconDto {
    pub mime: String,
    pub data: String,
}

impl Validate for SiteIconDto {
    fn validate(&self) -> Result<(), AppError> {
        if !SITE_ICON_MIMES.contains(&self.mime.as_str()) {
            return Err(invalid("mime", "enum"));
        }
        check::non_empty("data", &self.data)?;
        check::max_len("data", &self.data, SITE_ICON_DATA_MAX)
    }
}

pub type SiteIconOutput = Option<SiteIconDto>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate::parse;
    use serde_json::json;

    #[test]
    fn only_web_addresses_open_or_bring_an_icon() {
        assert!(parse::<OpenSiteInput>(json!({ "url": "https://example.com/a" })).is_ok());
        for url in [
            "file:///C:/x",
            "javascript:alert(1)",
            "data:text/html,x",
            "nope",
        ] {
            assert!(
                parse::<OpenSiteInput>(json!({ "url": url })).is_err(),
                "{url}"
            );
        }
        assert!(
            parse::<SiteIconInput>(json!({
                "url": "https://example.com",
                "icon": "https://cdn.example/logo.png"
            }))
            .is_ok()
        );
        assert!(
            parse::<SiteIconInput>(json!({ "url": "https://example.com", "icon": "ftp://x/y" }))
                .is_err()
        );
        assert!(
            parse::<SiteIconDto>(json!({ "mime": "image/svg+xml", "data": "PHN2Zz4=" })).is_err()
        );
    }
}
