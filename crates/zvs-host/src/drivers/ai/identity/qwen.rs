use async_trait::async_trait;
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use zvs_contract::{AccountFamily, AppError};

use super::{
    AccountIdentity, IdentityProbe, Payload, ProbeResult, fetch_identity, malformed_identity,
};
use crate::drivers::ai::transport::{AccountToken, DEFAULT_TOKEN_TYPE, SessionGateway};

pub const QWEN_IDENTITY_ENDPOINT: &str = "https://chat.qwen.ai/api/v1/auths/";
pub const QWEN_LOGIN_URL: &str = "https://chat.qwen.ai/";

const FAMILY: AccountFamily = AccountFamily::QwenWeb;

pub fn map_qwen_identity(payload: &Value) -> Result<ProbeResult, AppError> {
    parse(payload).ok_or_else(|| malformed_identity(FAMILY))
}

fn parse(payload: &Value) -> Option<ProbeResult> {
    let data = Payload::of(payload)?;
    let id = data.id("id").ok()?;
    let email = data.text("email").ok()?;
    let name = data.text("name").ok()?;
    let avatar = data.text("profile_image_url").ok()?;
    let token = data.text("token").ok()?;
    let token_type = data.text("token_type").ok()?;
    let expires_at = data.integer("expires_at").ok()?;
    if expires_at.is_some_and(|expires_at| expires_at <= 0) {
        return None;
    }
    let external_id = id.trim();
    if external_id.is_empty() {
        return None;
    }
    Some(ProbeResult {
        identity: AccountIdentity {
            external_id: external_id.to_owned(),
            email_masked: email,
            display_name: name,
            avatar_url: avatar,
            expires_at,
        },
        credential: token.map(|token| AccountToken {
            token,
            token_type: token_type.unwrap_or_else(|| DEFAULT_TOKEN_TYPE.to_owned()),
        }),
    })
}

#[derive(Debug, Clone, Copy, Default)]
pub struct QwenIdentityProbe;

#[async_trait]
impl IdentityProbe for QwenIdentityProbe {
    fn family(&self) -> AccountFamily {
        FAMILY
    }

    fn endpoint(&self) -> &str {
        QWEN_IDENTITY_ENDPOINT
    }

    fn login_url(&self) -> &str {
        QWEN_LOGIN_URL
    }

    async fn probe(
        &self,
        session: &dyn SessionGateway,
        cancel: &CancellationToken,
    ) -> Result<ProbeResult, AppError> {
        let credential = session.account_token(QWEN_IDENTITY_ENDPOINT);
        let payload = fetch_identity(
            FAMILY,
            QWEN_IDENTITY_ENDPOINT,
            session,
            cancel,
            credential.as_ref(),
        )
        .await?;
        let mut result = map_qwen_identity(&payload)?;
        result.credential = result.credential.or(credential);
        Ok(result)
    }
}
