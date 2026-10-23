use serde::{Deserialize, Serialize};

use crate::ids::ConversationId;
use crate::validate::no_refinements;

pub const NOTIFICATION_SCHEME: &str = "zvs-studio";
pub const NOTIFICATION_URL_MAX: usize = 256;
pub const NAVIGATION_STEP_DOMAIN: &str = "navigation";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum NotificationTargetDto {
    Conversation { conversation_id: ConversationId },
    Downloads,
}

no_refinements!(NotificationTargetDto);

impl NotificationTargetDto {
    pub const fn route(&self) -> &'static str {
        match self {
            Self::Conversation { .. } => "/chat",
            Self::Downloads => "/downloads",
        }
    }

    pub fn to_url(&self) -> String {
        match self {
            Self::Conversation { conversation_id } => {
                format!("{NOTIFICATION_SCHEME}://conversation/{conversation_id}")
            }
            Self::Downloads => format!("{NOTIFICATION_SCHEME}://downloads"),
        }
    }

    pub fn from_url(url: &str) -> Option<Self> {
        if url.len() > NOTIFICATION_URL_MAX {
            return None;
        }
        let rest = url.strip_prefix(NOTIFICATION_SCHEME)?.strip_prefix("://")?;
        let rest = rest.strip_suffix('/').unwrap_or(rest);
        match rest.split('/').collect::<Vec<_>>().as_slice() {
            ["downloads"] => Some(Self::Downloads),
            ["conversation", id] if id.len() == 36 => ConversationId::parse(id)
                .map(|conversation_id| Self::Conversation { conversation_id }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::validate::parse;

    const CONVERSATION: &str = "0199aa11-1111-7111-8111-111111111111";

    fn conversation() -> NotificationTargetDto {
        NotificationTargetDto::Conversation {
            conversation_id: ConversationId::parse(CONVERSATION).expect("uuid"),
        }
    }

    #[test]
    fn every_target_survives_a_url_round_trip() {
        for target in [conversation(), NotificationTargetDto::Downloads] {
            assert_eq!(
                NotificationTargetDto::from_url(&target.to_url()),
                Some(target)
            );
        }
        assert_eq!(
            conversation().to_url(),
            format!("zvs-studio://conversation/{CONVERSATION}")
        );
        assert_eq!(
            NotificationTargetDto::from_url("zvs-studio://downloads/"),
            Some(NotificationTargetDto::Downloads)
        );
    }

    #[test]
    fn anything_but_the_two_shapes_is_rejected() {
        for url in [
            "",
            "zvs-studio://",
            "zvs-studio:downloads",
            "https://downloads",
            "ZVS-STUDIO://downloads",
            "zvs-studio://evil/x",
            "zvs-studio://downloads/x",
            "zvs-studio://downloads?x=1",
            "zvs-studio://conversation/",
            "zvs-studio://conversation/not-a-uuid",
            "zvs-studio://conversation/0199aa11111171118111111111111111",
            &format!("zvs-studio://conversation/{CONVERSATION}/approve"),
            &format!("zvs-studio://conversation/{CONVERSATION}?approve=1"),
            &format!("zvs-studio://conversation/{CONVERSATION}#x"),
            &format!("zvs-studio://downloads{}", "/".repeat(300)),
        ] {
            assert_eq!(NotificationTargetDto::from_url(url), None, "{url}");
        }
    }

    #[test]
    fn targets_are_tagged_by_kind_on_the_wire() {
        assert_eq!(
            parse::<NotificationTargetDto>(json!({
                "kind": "conversation",
                "conversationId": CONVERSATION
            }))
            .expect("conversation"),
            conversation()
        );
        assert_eq!(
            serde_json::to_value(NotificationTargetDto::Downloads).expect("serialises"),
            json!({ "kind": "downloads" })
        );
        assert!(parse::<NotificationTargetDto>(json!({ "kind": "settings" })).is_err());
    }
}
