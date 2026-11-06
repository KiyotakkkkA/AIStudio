use serde::{Deserialize, Serialize};

use crate::enums::wire_enum;
use crate::validate::no_refinements;

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
}

impl Default for BrowserSettingsDto {
    fn default() -> Self {
        Self {
            enabled: true,
            web_links: BrowserLinkTarget::Studio,
            local_links: BrowserLinkTarget::Studio,
            full_url: false,
        }
    }
}

no_refinements!(BrowserSettingsDto);

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
            })
        );
    }
}
