use serde::{Deserialize, Serialize};

use crate::errors::AppError;
use crate::ids::is_wire_uuid;
use crate::primitives::Timestamp;
use crate::validate::{Validate, check, invalid};

pub const URL_MAX: usize = 8192;
pub const COOKIE_ID_MAX: usize = 32768;
pub const DOMAIN_MAX: usize = 253;
pub const NETWORK_ENTRY_ID_MAX: usize = 256;

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
    Devtools {
        visible: bool,
    },
    Network,
    NetworkRecord {
        recording: bool,
    },
    NetworkClear,
    NetworkSave,
    NetworkDetail {
        id: String,
    },
    Agent {
        visible: bool,
    },
    AgentApprove {
        id: String,
    },
    AgentAlways {
        id: String,
    },
    AgentDeny {
        id: String,
    },
    AgentControl,
    AgentResume,
    AgentStop,
    AgentRevoke {
        id: String,
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
            Self::AgentApprove { id }
            | Self::AgentAlways { id }
            | Self::AgentDeny { id }
            | Self::AgentRevoke { id } => tab_id(id),
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
            Self::NetworkDetail { id } => {
                check::non_empty("id", id)?;
                check::max_len("id", id, NETWORK_ENTRY_ID_MAX)
            }
            Self::State
            | Self::Back
            | Self::Forward
            | Self::Reload
            | Self::Sites { .. }
            | Self::Cookies
            | Self::Devtools { .. }
            | Self::Network
            | Self::NetworkRecord { .. }
            | Self::NetworkClear
            | Self::NetworkSave
            | Self::Agent { .. }
            | Self::AgentControl
            | Self::AgentResume
            | Self::AgentStop => Ok(()),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BrowserAgentStatus {
    #[default]
    Idle,
    Running,
    Approval,
    Paused,
    Control,
    Stopped,
    Finished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum BrowserAuthorisation {
    Auto,
    Granted { at: Timestamp },
    Approved { at: Timestamp },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BrowserActionState {
    Running,
    Waiting,
    Done,
    Failed,
    Denied,
    Paused,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserAgentAction {
    pub id: u64,
    pub tool: String,
    pub target: String,
    pub state: BrowserActionState,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub authorisation: Option<BrowserAuthorisation>,
    pub started_at: Timestamp,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub duration_ms: Option<u64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserAgentApproval {
    pub id: String,
    pub tool: String,
    pub origin: String,
    pub target: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub text: Option<String>,
    pub can_always: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSiteGrant {
    pub id: String,
    pub tool: String,
    pub granted_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserAgentState {
    pub status: BrowserAgentStatus,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub run_id: Option<String>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub task: Option<String>,
    pub steps: u64,
    pub pace: u64,
    pub limit: u64,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub target: Option<String>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub approval: Option<BrowserAgentApproval>,
    pub actions: Vec<BrowserAgentAction>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub origin: Option<String>,
    pub grants: Vec<BrowserSiteGrant>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserState {
    pub tabs: Vec<BrowserTab>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub active_id: Option<String>,
    pub sites_visible: bool,
    pub devtools_visible: bool,
    pub agent_visible: bool,
    pub agent: Box<BrowserAgentState>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BrowserRequestState {
    Pending,
    Finished,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserHeader {
    pub name: String,
    pub value: String,
    pub redacted: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserNetworkEntry {
    pub id: String,
    pub url: String,
    pub method: String,
    pub resource_type: String,
    pub state: BrowserRequestState,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub status: Option<u16>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub mime_type: Option<String>,
    pub started_at: f64,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub duration: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub size: Option<f64>,
    pub from_cache: bool,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserNetworkLog {
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub tab_id: Option<String>,
    pub recording: bool,
    pub revision: u64,
    pub dropped: u64,
    pub entries: Vec<BrowserNetworkEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserNetworkBody {
    pub text: String,
    pub base64: bool,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserNetworkDetail {
    pub entry: BrowserNetworkEntry,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub status_text: Option<String>,
    pub request_headers: Vec<BrowserHeader>,
    pub response_headers: Vec<BrowserHeader>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub post_data: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub body: Option<BrowserNetworkBody>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserNetworkSaved {
    pub saved: bool,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub file_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BrowserCommandOutput {
    State(BrowserState),
    Cookies(Vec<BrowserCookie>),
    Network(BrowserNetworkLog),
    NetworkDetail(Box<BrowserNetworkDetail>),
    NetworkSaved(BrowserNetworkSaved),
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
            parse::<BrowserCommand>(json!({ "action": "networkRecord", "recording": true }))
                .expect("parses"),
            BrowserCommand::NetworkRecord { recording: true }
        );
        assert_eq!(
            parse::<BrowserCommand>(json!({ "action": "networkClear" })).expect("parses"),
            BrowserCommand::NetworkClear
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
            json!({ "action": "networkDetail", "id": "" }),
            json!({ "action": "networkDetail", "id": "a".repeat(NETWORK_ENTRY_ID_MAX + 1) }),
            json!({ "action": "networkRecord" }),
            json!({ "action": "devtools" }),
            json!({ "action": "agentApprove", "id": "not-an-approval" }),
            json!({ "action": "agentAlways" }),
            json!({ "action": "agentRevoke", "id": "" }),
            json!({ "action": "agent" }),
            json!({}),
        ] {
            assert!(parse::<BrowserCommand>(value.clone()).is_err(), "{value}");
        }
    }

    #[test]
    fn an_agent_action_carries_its_authorisation() {
        let action = BrowserAgentAction {
            id: 3,
            tool: "browser.click".into(),
            target: "button «Отправить»".into(),
            state: BrowserActionState::Done,
            authorisation: Some(BrowserAuthorisation::Granted {
                at: 1_800_000_000_000,
            }),
            started_at: 1_800_000_000_500,
            duration_ms: Some(42),
            error: None,
        };
        assert_eq!(
            serde_json::to_value(&action).expect("serialises"),
            json!({
                "id": 3,
                "tool": "browser.click",
                "target": "button «Отправить»",
                "state": "done",
                "authorisation": { "kind": "granted", "at": 1_800_000_000_000_i64 },
                "startedAt": 1_800_000_000_500_i64,
                "durationMs": 42,
            })
        );
        assert_eq!(
            serde_json::to_value(BrowserAuthorisation::Auto).expect("serialises"),
            json!({ "kind": "auto" })
        );
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
            devtools_visible: true,
            agent_visible: false,
            agent: Box::default(),
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
                "devtoolsVisible": true,
                "agentVisible": false,
                "agent": {
                    "status": "idle",
                    "runId": null,
                    "task": null,
                    "steps": 0,
                    "pace": 0,
                    "limit": 0,
                    "target": null,
                    "approval": null,
                    "actions": [],
                    "origin": null,
                    "grants": [],
                },
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

    #[test]
    fn a_network_log_keeps_a_null_tab_and_omits_absent_response_fields() {
        let log = BrowserNetworkLog {
            tab_id: None,
            recording: true,
            revision: 3,
            dropped: 0,
            entries: vec![BrowserNetworkEntry {
                id: "17.2".into(),
                url: "https://a.example/api".into(),
                method: "GET".into(),
                resource_type: "Fetch".into(),
                state: BrowserRequestState::Pending,
                status: None,
                mime_type: None,
                started_at: 1_800_000_000_000.0,
                duration: None,
                size: None,
                from_cache: false,
                error: None,
            }],
        };
        assert_eq!(
            serde_json::to_value(BrowserCommandOutput::Network(log)).expect("serialises"),
            json!({
                "tabId": null,
                "recording": true,
                "revision": 3,
                "dropped": 0,
                "entries": [{
                    "id": "17.2",
                    "url": "https://a.example/api",
                    "method": "GET",
                    "resourceType": "Fetch",
                    "state": "pending",
                    "startedAt": 1_800_000_000_000.0,
                    "fromCache": false,
                }],
            })
        );
    }
}
