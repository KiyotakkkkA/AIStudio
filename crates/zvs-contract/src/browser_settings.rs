use serde::{Deserialize, Serialize};

use crate::browser_data::DOWNLOAD_PATH_MAX;
use crate::enums::wire_enum;
use crate::errors::AppError;
use crate::validate::{Validate, check};

wire_enum!(BrowserLinkTarget {
    Studio => "studio",
    System => "system",
});

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSettingsDto {
    pub enabled: bool,
    pub web_links: BrowserLinkTarget,
    pub local_links: BrowserLinkTarget,
    pub full_url: bool,
    pub password_autosave: bool,
    pub form_autofill: bool,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub download_dir: Option<String>,
    pub ask_download: bool,
}

impl Default for BrowserSettingsDto {
    fn default() -> Self {
        Self {
            enabled: true,
            web_links: BrowserLinkTarget::Studio,
            local_links: BrowserLinkTarget::Studio,
            full_url: false,
            password_autosave: false,
            form_autofill: true,
            download_dir: None,
            ask_download: false,
        }
    }
}

impl Validate for BrowserSettingsDto {
    fn validate(&self) -> Result<(), AppError> {
        if let Some(dir) = &self.download_dir {
            check::non_empty("downloadDir", dir)?;
            check::max_len("downloadDir", dir, DOWNLOAD_PATH_MAX)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn settings_travel_in_camel_case() {
        assert_eq!(
            serde_json::to_value(BrowserSettingsDto::default()).expect("serialises"),
            json!({
                "enabled": true,
                "webLinks": "studio",
                "localLinks": "studio",
                "fullUrl": false,
                "passwordAutosave": false,
                "formAutofill": true,
                "downloadDir": null,
                "askDownload": false,
            })
        );
    }
}
