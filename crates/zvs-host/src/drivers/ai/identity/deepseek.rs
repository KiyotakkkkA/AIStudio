use async_trait::async_trait;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use zvs_contract::{AccountFamily, AppError};

use super::{
    AccountIdentity, IdentityProbe, Payload, ProbeResult, family_details, fetch_identity,
    malformed_identity,
};
use crate::drivers::ai::errors::session_expired;
use crate::drivers::ai::transport::{AccountToken, SessionGateway};

pub const DEEPSEEK_IDENTITY_ENDPOINT: &str = "https://chat.deepseek.com/api/v0/users/current";
pub const DEEPSEEK_LOGIN_URL: &str = "https://chat.deepseek.com/";

const FAMILY: AccountFamily = AccountFamily::DeepseekWeb;

struct Biz {
    id: String,
    email: Option<String>,
    token: Option<String>,
    name: Option<String>,
    picture: Option<String>,
}

struct Parsed {
    code: Option<i64>,
    biz_code: Option<i64>,
    biz: Option<Biz>,
}

fn parse(payload: &Value) -> Option<Parsed> {
    let root = Payload::of(payload)?;
    let code = root.integer("code").ok()?;
    root.text("msg").ok()?;
    let Some(data) = root.nested("data").ok()? else {
        return Some(Parsed {
            code,
            biz_code: None,
            biz: None,
        });
    };
    let biz_code = data.integer("biz_code").ok()?;
    data.text("biz_msg").ok()?;
    let biz = match data.nested("biz_data").ok()? {
        None => None,
        Some(biz) => {
            let profile = biz.nested("id_profile").ok()?;
            let (name, picture) = match profile {
                Some(profile) => (profile.text("name").ok()?, profile.text("picture").ok()?),
                None => (None, None),
            };
            Some(Biz {
                id: biz.id("id").ok()?,
                email: biz.text("email").ok()?,
                token: biz.text("token").ok()?,
                name,
                picture,
            })
        }
    };
    Some(Parsed {
        code,
        biz_code,
        biz,
    })
}

pub fn map_deepseek_identity(payload: &Value) -> Result<ProbeResult, AppError> {
    let parsed = parse(payload).ok_or_else(|| malformed_identity(FAMILY))?;
    if parsed.code.is_some_and(|code| code != 0) || parsed.biz_code.is_some_and(|code| code != 0) {
        let mut details = family_details(FAMILY);
        details.insert("code".to_owned(), json!(parsed.code.unwrap_or(0)));
        details.insert("bizCode".to_owned(), json!(parsed.biz_code.unwrap_or(0)));
        return Err(session_expired(details));
    }
    let Some(biz) = parsed.biz else {
        let mut details = family_details(FAMILY);
        details.insert("reason".to_owned(), json!("no-profile"));
        return Err(session_expired(details));
    };
    let external_id = biz.id.trim();
    if external_id.is_empty() {
        return Err(malformed_identity(FAMILY));
    }
    Ok(ProbeResult {
        identity: AccountIdentity {
            external_id: external_id.to_owned(),
            email_masked: biz.email,
            display_name: biz.name,
            avatar_url: biz.picture,
            expires_at: None,
        },
        credential: biz.token.map(AccountToken::bearer),
    })
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DeepSeekIdentityProbe;

#[async_trait]
impl IdentityProbe for DeepSeekIdentityProbe {
    fn family(&self) -> AccountFamily {
        FAMILY
    }

    fn endpoint(&self) -> &str {
        DEEPSEEK_IDENTITY_ENDPOINT
    }

    fn login_url(&self) -> &str {
        DEEPSEEK_LOGIN_URL
    }

    async fn probe(
        &self,
        session: &dyn SessionGateway,
        cancel: &CancellationToken,
    ) -> Result<ProbeResult, AppError> {
        let credential = session.account_token(DEEPSEEK_IDENTITY_ENDPOINT);
        let payload = fetch_identity(
            FAMILY,
            DEEPSEEK_IDENTITY_ENDPOINT,
            session,
            cancel,
            credential.as_ref(),
        )
        .await?;
        let mut result = map_deepseek_identity(&payload)?;
        result.credential = result.credential.or(credential);
        Ok(result)
    }
}
