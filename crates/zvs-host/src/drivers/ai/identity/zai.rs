use async_trait::async_trait;
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use zvs_contract::{AccountFamily, AppError};

use super::{IdentityProbe, ProbeResult, fetch_identity, malformed_identity, qwen};
use crate::drivers::ai::transport::SessionGateway;

pub const ZAI_IDENTITY_ENDPOINT: &str = "https://chat.z.ai/api/v1/auths/";
pub const ZAI_LOGIN_URL: &str = "https://chat.z.ai/";

const FAMILY: AccountFamily = AccountFamily::ZaiWeb;

pub fn map_zai_identity(payload: &Value) -> Result<ProbeResult, AppError> {
    qwen::parse(payload).ok_or_else(|| malformed_identity(FAMILY))
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ZaiIdentityProbe;

#[async_trait]
impl IdentityProbe for ZaiIdentityProbe {
    fn family(&self) -> AccountFamily {
        FAMILY
    }

    fn endpoint(&self) -> &str {
        ZAI_IDENTITY_ENDPOINT
    }

    fn login_url(&self) -> &str {
        ZAI_LOGIN_URL
    }

    async fn probe(
        &self,
        session: &dyn SessionGateway,
        cancel: &CancellationToken,
    ) -> Result<ProbeResult, AppError> {
        let credential = session.account_token(ZAI_IDENTITY_ENDPOINT);
        let payload = fetch_identity(
            FAMILY,
            ZAI_IDENTITY_ENDPOINT,
            session,
            cancel,
            credential.as_ref(),
        )
        .await?;
        let mut result = map_zai_identity(&payload)?;
        result.credential = result.credential.or(credential);
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use zvs_contract::AppErrorCode;

    use super::*;
    use crate::drivers::ai::identity::AccountIdentity;
    use crate::drivers::ai::transport::AccountToken;

    #[test]
    fn the_observed_profile_maps_to_an_identity_and_its_bearer() {
        let result = map_zai_identity(&json!({
            "id": " b9856270-3772-4fb5-864f-384f57a72cc2 ",
            "email": "person@example.com",
            "name": "Петр",
            "profile_image_url": "https://example.com/a.png",
            "role": "user",
            "idp": "email",
            "phone_num": null,
            "token": "zai-token",
            "token_type": "Bearer",
            "expires_at": null,
            "permissions": { "chat": { "edit": true } },
        }))
        .expect("maps");
        assert_eq!(
            result.identity,
            AccountIdentity {
                external_id: "b9856270-3772-4fb5-864f-384f57a72cc2".to_owned(),
                email_masked: Some("person@example.com".to_owned()),
                display_name: Some("Петр".to_owned()),
                avatar_url: Some("https://example.com/a.png".to_owned()),
                expires_at: None,
            }
        );
        assert_eq!(result.credential, Some(AccountToken::bearer("zai-token")));
        let error =
            map_zai_identity(&json!({ "detail": "Not authenticated" })).expect_err("refuses");
        assert_eq!(error.code, AppErrorCode::ValidationFailed);
        assert_eq!(
            error
                .details
                .and_then(|details| details.get("family").cloned()),
            Some(json!("zai-web"))
        );
        assert_eq!(ZaiIdentityProbe.endpoint(), ZAI_IDENTITY_ENDPOINT);
        assert_eq!(ZaiIdentityProbe.login_url(), "https://chat.z.ai/");
    }
}
