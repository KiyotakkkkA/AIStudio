use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::validate::no_refinements;

macro_rules! branded_ids {
    ($($name:ident),+ $(,)?) => {$(
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(#[serde(with = "wire_uuid")] Uuid);

        impl $name {
            pub const fn new(value: Uuid) -> Self {
                Self(value)
            }

            pub fn parse(value: &str) -> Option<Self> {
                wire_uuid::parse(value).map(Self)
            }

            pub const fn as_uuid(&self) -> &Uuid {
                &self.0
            }
        }

        impl From<Uuid> for $name {
            fn from(value: Uuid) -> Self {
                Self(value)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(&self.0.hyphenated(), formatter)
            }
        }

        #[cfg(feature = "schema")]
        impl schemars::JsonSchema for $name {
            fn inline_schema() -> bool {
                true
            }

            fn schema_name() -> std::borrow::Cow<'static, str> {
                stringify!($name).into()
            }

            fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
                schemars::json_schema!({ "type": "string", "format": "uuid" })
            }
        }

        no_refinements!($name);
    )+};
}

branded_ids!(
    SecretId,
    IdentityId,
    ProviderId,
    ModelId,
    VectorStoreId,
    DocumentId,
    GraphRelationTypeId,
    GraphEdgeId,
    ConversationId,
    MessageId,
    ScenarioId,
    RunId,
    StepId,
    SkillId,
    McpServerId,
    ToolId,
    IntegrationId,
    DownloadId,
    TaskId,
    StreamId,
    AccountId,
    VectorEvalCaseId,
);

pub fn is_wire_uuid(value: &str) -> bool {
    wire_uuid::parse(value).is_some()
}

mod wire_uuid {
    use serde::de::Error as _;
    use serde::{Deserialize, Deserializer, Serializer};
    use uuid::Uuid;

    const NIL: &str = "00000000-0000-0000-0000-000000000000";
    const MAX: &str = "ffffffff-ffff-ffff-ffff-ffffffffffff";

    pub fn parse(value: &str) -> Option<Uuid> {
        let bytes = value.as_bytes();
        if bytes.len() != 36 {
            return None;
        }
        let shaped = bytes.iter().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => *byte == b'-',
            _ => byte.is_ascii_hexdigit(),
        });
        let versioned = (b'1'..=b'8').contains(&bytes[14])
            && matches!(bytes[19], b'8' | b'9' | b'a' | b'b' | b'A' | b'B');
        if shaped && (versioned || value == NIL || value == MAX) {
            Uuid::parse_str(value).ok()
        } else {
            None
        }
    }

    pub fn serialize<S: Serializer>(value: &Uuid, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&value.hyphenated())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Uuid, D::Error> {
        let text = String::deserialize(deserializer)?;
        parse(&text).ok_or_else(|| D::Error::custom(format!("invalid UUID: {text}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const V7: &str = "0199aa11-1111-7111-8111-111111111111";

    #[test]
    fn an_id_is_a_bare_uuid_string_on_the_wire() {
        let id: ProviderId = serde_json::from_value(json!(V7)).expect("parses");
        assert_eq!(serde_json::to_value(id).expect("serialises"), json!(V7));
        assert_eq!(id.to_string(), V7);
    }

    #[test]
    fn only_the_hyphenated_rfc_form_is_accepted() {
        for raw in [
            "0199aa11111171118111111111111111",
            "{0199aa11-1111-7111-8111-111111111111}",
            "urn:uuid:0199aa11-1111-7111-8111-111111111111",
            "0199aa11-1111-0111-8111-111111111111",
            "0199aa11-1111-7111-c111-111111111111",
            "nope",
        ] {
            assert!(
                serde_json::from_value::<StreamId>(json!(raw)).is_err(),
                "{raw}"
            );
        }
        assert!(serde_json::from_value::<StreamId>(json!(1)).is_err());
    }

    #[test]
    fn nil_and_max_are_accepted_like_zod() {
        assert!(StreamId::parse("00000000-0000-0000-0000-000000000000").is_some());
        assert!(StreamId::parse("ffffffff-ffff-ffff-ffff-ffffffffffff").is_some());
    }
}
