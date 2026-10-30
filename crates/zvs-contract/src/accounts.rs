use serde::{Deserialize, Serialize};

use crate::ai::{AccountFamily, AccountStatus};
use crate::errors::AppError;
use crate::ids::AccountId;
use crate::validate::{Validate, check, nested, no_refinements};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountDto {
    pub id: AccountId,
    pub adapter: AccountFamily,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub email_masked: Option<String>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub display_name: Option<String>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub avatar_url: Option<String>,
    pub status: AccountStatus,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub detail: Option<String>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub expires_at: Option<i64>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub last_checked_at: Option<i64>,
    pub linked_providers_count: u64,
}

impl Validate for AccountDto {
    fn validate(&self) -> Result<(), AppError> {
        check::safe_integer("linkedProvidersCount", self.linked_providers_count)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountLinkInput {
    pub adapter: AccountFamily,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub account_id: Option<AccountId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountRef {
    pub id: AccountId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "status",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum AccountLinkResult {
    Linked {
        account: AccountDto,
        replaced_previous_identity: bool,
        #[serde(deserialize_with = "crate::wire::nullable")]
        detail: Option<String>,
    },
    Cancelled {
        detail: String,
    },
    Timeout {
        detail: String,
    },
}

impl Validate for AccountLinkResult {
    fn validate(&self) -> Result<(), AppError> {
        match self {
            Self::Linked { account, .. } => nested("account", account.validate()),
            Self::Cancelled { .. } | Self::Timeout { .. } => Ok(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsCancelLinkOutput {
    pub cancelled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsUnlinkOutput {
    pub id: AccountId,
    pub removed: bool,
    pub browser_session_preserved: bool,
}

impl Validate for AccountsUnlinkOutput {
    fn validate(&self) -> Result<(), AppError> {
        check::literal_true("removed", self.removed)?;
        check::literal_true("browserSessionPreserved", self.browser_session_preserved)
    }
}

pub type AccountsListOutput = Vec<AccountDto>;

no_refinements!(AccountLinkInput, AccountRef, AccountsCancelLinkOutput);

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::validate::parse;

    const ID: &str = "0199c3a8-6b2f-7c1e-9a4d-1f2e3d4c5b6a";

    fn account() -> serde_json::Value {
        json!({
            "id": ID,
            "adapter": "qwen-web",
            "emailMasked": "p***@example.com",
            "displayName": "Person",
            "avatarUrl": null,
            "status": "linked",
            "detail": null,
            "expiresAt": 2_000_000_000_000_i64,
            "lastCheckedAt": 1_700_000_000_000_i64,
            "linkedProvidersCount": 1,
        })
    }

    #[test]
    fn a_link_result_is_tagged_by_status_and_round_trips() {
        let linked = json!({
            "status": "linked",
            "account": account(),
            "replacedPreviousIdentity": false,
            "detail": null,
        });
        let parsed = parse::<AccountLinkResult>(linked.clone()).expect("parses");
        assert_eq!(serde_json::to_value(&parsed).expect("serialises"), linked);
        let timeout = json!({ "status": "timeout", "detail": "No sign-in detected" });
        assert_eq!(
            parse::<AccountLinkResult>(timeout).expect("parses"),
            AccountLinkResult::Timeout {
                detail: "No sign-in detected".to_owned()
            }
        );
        assert!(parse::<AccountLinkResult>(json!({ "status": "linked", "detail": null })).is_err());
        assert!(parse::<AccountLinkResult>(json!({ "status": "gone", "detail": "x" })).is_err());
    }

    #[test]
    fn an_unlink_output_insists_on_both_literals() {
        assert!(
            parse::<AccountsUnlinkOutput>(json!({
                "id": ID,
                "removed": true,
                "browserSessionPreserved": true,
            }))
            .is_ok()
        );
        assert!(
            parse::<AccountsUnlinkOutput>(json!({
                "id": ID,
                "removed": true,
                "browserSessionPreserved": false,
            }))
            .is_err()
        );
        let mut missing = account();
        missing.as_object_mut().expect("an object").remove("detail");
        assert!(parse::<AccountDto>(missing).is_err());
    }
}
