use serde::{Deserialize, Serialize};

use crate::errors::AppError;
use crate::ids::is_wire_uuid;
use crate::validate::{Validate, check, invalid};

pub const URL_MAX: usize = 8192;
pub const COOKIE_ID_MAX: usize = 32768;
pub const DOMAIN_MAX: usize = 253;

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Validate for BrowserBounds {
    fn validate(&self) -> Result<(), AppError> {
        for (field, value) in [
            ("x", self.x),
            ("y", self.y),
            ("width", self.width),
            ("height", self.height),
        ] {
            check::number_range(field, value, 0.0, f64::MAX)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum BrowserCommand {
    State,
    New {
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "crate::wire::present"
        )]
        url: Option<String>,
    },
    Navigate {
        url: String,
    },
    Switch {
        id: String,
    },
    Close {
        id: String,
    },
    Reorder {
        id: String,
        index: u64,
    },
    Back,
    Forward,
    Reload,
    Sites {
        visible: bool,
    },
    Cookies,
    RemoveCookie {
        id: String,
    },
    ClearSite {
        domain: String,
    },
}

fn tab_id(id: &str) -> Result<(), AppError> {
    if is_wire_uuid(id) {
        Ok(())
    } else {
        Err(invalid("id", "uuid"))
    }
}

impl Validate for BrowserCommand {
    fn validate(&self) -> Result<(), AppError> {
        match self {
            Self::New { url } => url
                .as_deref()
                .map_or(Ok(()), |url| check::max_len("url", url, URL_MAX)),
            Self::Navigate { url } => {
                check::non_empty("url", url)?;
                check::max_len("url", url, URL_MAX)
            }
            Self::Switch { id } | Self::Close { id } => tab_id(id),
            Self::Reorder { id, index } => {
                tab_id(id)?;
                check::safe_integer("index", *index)
            }
            Self::RemoveCookie { id } => {
                check::non_empty("id", id)?;
                check::max_len("id", id, COOKIE_ID_MAX)
            }
            Self::ClearSite { domain } => {
                check::non_empty("domain", domain)?;
                check::max_len("domain", domain, DOMAIN_MAX)
            }
            Self::State
            | Self::Back
            | Self::Forward
            | Self::Reload
            | Self::Sites { .. }
            | Self::Cookies => Ok(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BrowserSecurity {
    Https,
    Http,
    None,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserTab {
    pub id: String,
    pub title: String,
    pub url: String,
    pub loading: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub security: BrowserSecurity,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserState {
    pub tabs: Vec<BrowserTab>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub active_id: Option<String>,
    pub sites_visible: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserCookie {
    pub id: String,
    pub domain: String,
    pub name: String,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    pub session: bool,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub expiration_date: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BrowserCommandOutput {
    State(BrowserState),
    Cookies(Vec<BrowserCookie>),
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::validate::parse;

    const TAB: &str = "0199aa11-1111-7111-8111-111111111111";

    #[test]
    fn commands_use_the_action_tag() {
        assert_eq!(
            parse::<BrowserCommand>(json!({ "action": "removeCookie", "id": "x" }))
                .expect("parses"),
            BrowserCommand::RemoveCookie { id: "x".into() }
        );
        assert_eq!(
            parse::<BrowserCommand>(json!({ "action": "new" })).expect("parses"),
            BrowserCommand::New { url: None }
        );
        assert_eq!(
            parse::<BrowserCommand>(json!({ "action": "reorder", "id": TAB, "index": 2 }))
                .expect("parses"),
            BrowserCommand::Reorder {
                id: TAB.into(),
                index: 2
            }
        );
    }

    #[test]
    fn unknown_commands_and_malformed_mutations_are_refused() {
        for value in [
            json!({ "action": "execute", "script": "alert(1)" }),
            json!({ "action": "clearSite", "domain": "" }),
            json!({ "action": "reorder", "id": "not-a-tab", "index": -1 }),
            json!({ "action": "reorder", "id": TAB, "index": -1 }),
            json!({ "action": "switch", "id": "not-a-tab" }),
            json!({ "action": "navigate", "url": "" }),
            json!({ "action": "navigate", "url": "a".repeat(URL_MAX + 1) }),
            json!({ "action": "new", "url": null }),
            json!({ "action": "clearSite", "domain": "a".repeat(DOMAIN_MAX + 1) }),
            json!({}),
        ] {
            assert!(parse::<BrowserCommand>(value.clone()).is_err(), "{value}");
        }
    }

    #[test]
    fn bounds_are_finite_and_non_negative() {
        assert!(
            parse::<BrowserBounds>(json!({ "x": 0, "y": 0.5, "width": 10, "height": 20 })).is_ok()
        );
        assert!(
            parse::<BrowserBounds>(json!({ "x": -1, "y": 0, "width": 10, "height": 20 })).is_err()
        );
        assert!(parse::<BrowserBounds>(json!({ "x": 0, "y": 0, "width": 10 })).is_err());
    }

    #[test]
    fn state_serialises_a_null_active_tab_and_omits_absent_errors() {
        let state = BrowserState {
            tabs: vec![BrowserTab {
                id: TAB.into(),
                title: "Новая вкладка".into(),
                url: String::new(),
                loading: false,
                can_go_back: false,
                can_go_forward: false,
                security: BrowserSecurity::None,
                error: None,
            }],
            active_id: None,
            sites_visible: false,
        };
        assert_eq!(
            serde_json::to_value(BrowserCommandOutput::State(state)).expect("serialises"),
            json!({
                "tabs": [{
                    "id": TAB,
                    "title": "Новая вкладка",
                    "url": "",
                    "loading": false,
                    "canGoBack": false,
                    "canGoForward": false,
                    "security": "none",
                }],
                "activeId": null,
                "sitesVisible": false,
            })
        );
    }

    #[test]
    fn cookies_carry_metadata_only() {
        let cookie = BrowserCookie {
            id: "[\"a.example\",\"/\",\"sid\",true]".into(),
            domain: "a.example".into(),
            name: "sid".into(),
            path: "/".into(),
            secure: true,
            http_only: true,
            session: false,
            expiration_date: Some(1_800_000_000.0),
        };
        let value =
            serde_json::to_value(BrowserCommandOutput::Cookies(vec![cookie])).expect("serialises");
        assert_eq!(
            value,
            json!([{
                "id": "[\"a.example\",\"/\",\"sid\",true]",
                "domain": "a.example",
                "name": "sid",
                "path": "/",
                "secure": true,
                "httpOnly": true,
                "session": false,
                "expirationDate": 1_800_000_000.0,
            }])
        );
        assert!(value[0].get("value").is_none());
    }
}
