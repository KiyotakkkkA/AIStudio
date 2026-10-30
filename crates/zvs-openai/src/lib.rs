use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use url::Url;

pub const AUTHORIZE_URL: &str = "https://auth.openai.com/api/accounts/authorize";
pub const TOKEN_URL: &str = "https://auth.openai.com/api/accounts/oauth/token";
pub const API_BASE: &str = "https://api.openai.com/v1";
pub const SCOPES: &str =
    "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct";
pub const DIRECT_SCOPE: &str = "chatgpt.tokens.use.direct";
pub const APP_ID: &str = "ZVSAIStudio";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallbackError {
    Invalid,
    Denied,
}

pub fn callback(
    url: &Url,
    state: &str,
    registered: Option<&str>,
) -> Result<(String, String), CallbackError> {
    let mut params = std::collections::HashMap::new();
    for (key, value) in url.query_pairs() {
        if params.insert(key, value).is_some() {
            return Err(CallbackError::Invalid);
        }
    }
    if url.path() != "/auth/callback"
        || params.get("state").map(|value| value.as_ref()) != Some(state)
    {
        return Err(CallbackError::Invalid);
    }
    if params.contains_key("error") {
        return Err(CallbackError::Denied);
    }
    let code = params
        .get("code")
        .filter(|code| !code.is_empty())
        .ok_or(CallbackError::Invalid)?
        .to_string();
    let supplied = params.get("client_id").map(|value| value.as_ref());
    let client = match (registered, supplied) {
        (Some(expected), Some(actual)) if expected != actual => return Err(CallbackError::Invalid),
        (Some(expected), _) => expected,
        (None, Some(actual)) if !actual.is_empty() && actual != "dynamic_agent_client" => actual,
        _ => return Err(CallbackError::Invalid),
    };
    Ok((code, client.to_owned()))
}

pub fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

pub fn authorization_url(
    host: &str,
    redirect: &str,
    state: &str,
    nonce: &str,
    verifier: &str,
    registration: Option<(&str, &str)>,
) -> Url {
    let mut url = Url::parse(AUTHORIZE_URL).expect("static URL");
    let mut query = url.query_pairs_mut();
    query.extend_pairs([
        (
            "client_id",
            registration.map_or("dynamic_agent_client", |(id, _)| id),
        ),
        ("ext_agent_host_id", host),
        ("redirect_uri", redirect),
        ("response_type", "code"),
        ("scope", SCOPES),
        ("resource", API_BASE),
        ("state", state),
        ("nonce", nonce),
        ("code_challenge_method", "S256"),
        ("code_challenge", &pkce_challenge(verifier)),
    ]);
    match registration {
        Some((_, token)) => {
            query.append_pair("id_token_hint", token);
        }
        None => {
            query.append_pair("agent_name_hint", APP_ID);
        }
    }
    drop(query);
    url
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub client_id: String,
    pub host_id: String,
    pub subject: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub access_token: String,
    pub refresh_token: String,
    pub id_token: String,
    pub scope: String,
    pub expires_at: i64,
    #[serde(default)]
    pub earliest_refresh_at: Option<i64>,
}

impl Credentials {
    pub fn allows_inference(&self) -> bool {
        self.scope
            .split_whitespace()
            .any(|scope| scope == DIRECT_SCOPE)
    }
    pub fn identity_key(&self) -> String {
        format!("{}:{}", self.client_id, self.subject)
    }
}

#[derive(Clone, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: String,
    #[serde(default)]
    pub id_token: Option<String>,
    pub scope: String,
    pub expires_in: i64,
    pub token_type: String,
    #[serde(default)]
    pub earliest_refresh_at: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Model {
    pub slug: String,
    pub display_name: String,
    pub visibility: String,
    #[serde(default)]
    pub context_window: Option<i64>,
}

pub fn models(value: Value) -> Result<Vec<Model>, serde_json::Error> {
    #[derive(Deserialize)]
    struct Catalog {
        models: Vec<Model>,
    }
    let catalog: Catalog = serde_json::from_value(value)?;
    Ok(catalog
        .models
        .into_iter()
        .filter(|model| model.visibility == "list" && !model.slug.is_empty())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn pkce_uses_the_rfc7636_vector() {
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }
    #[test]
    fn new_and_returning_registrations_have_distinct_parameters() {
        let new = authorization_url(
            "urn:uuid:host",
            "http://127.0.0.1:123/auth/callback",
            "state",
            "nonce",
            "verifier",
            None,
        );
        let pairs = new
            .query_pairs()
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(pairs["client_id"], "dynamic_agent_client");
        assert_eq!(pairs["agent_name_hint"], APP_ID);
        let returning = authorization_url(
            "urn:uuid:host",
            "http://127.0.0.1:123/auth/callback",
            "state",
            "nonce",
            "verifier",
            Some(("issued", "hint")),
        );
        let pairs = returning
            .query_pairs()
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(pairs["client_id"], "issued");
        assert_eq!(pairs["id_token_hint"], "hint");
        assert!(!pairs.contains_key("agent_name_hint"));
    }
    #[test]
    fn catalog_preserves_server_order_and_hides_non_list_models() {
        let listed = models(json!({"models":[
            {"slug":"b", "display_name":"B", "visibility":"list"},
            {"slug":"hidden", "display_name":"Hidden", "visibility":"hide"},
            {"slug":"a", "display_name":"A", "visibility":"list"}
        ]}))
        .expect("catalog");
        assert_eq!(
            listed.iter().map(|m| m.slug.as_str()).collect::<Vec<_>>(),
            ["b", "a"]
        );
        assert!(models(json!({"data":[]})).is_err());
    }

    #[test]
    fn callbacks_require_state_and_preserve_the_selected_registration() {
        let parse = |query: &str| {
            Url::parse(&format!("http://127.0.0.1:1455/auth/callback?{query}")).expect("URL")
        };
        assert!(
            callback(
                &parse("code=c&state=wrong&client_id=issued"),
                "expected",
                None
            )
            .is_err()
        );
        assert!(
            callback(
                &parse("code=c&state=expected&client_id=other"),
                "expected",
                Some("issued")
            )
            .is_err()
        );
        assert!(callback(&parse("code=c&state=expected"), "expected", None).is_err());
        assert!(
            callback(
                &parse("code=c&state=expected&client_id=dynamic_agent_client"),
                "expected",
                None
            )
            .is_err()
        );
        assert!(
            callback(
                &parse("code=c&state=expected&state=other&client_id=issued"),
                "expected",
                None
            )
            .is_err()
        );
        assert_eq!(
            callback(
                &parse("error=access_denied&state=expected"),
                "expected",
                None
            ),
            Err(CallbackError::Denied)
        );
        assert_eq!(
            callback(&parse("code=c&state=expected"), "expected", Some("issued")),
            Ok(("c".to_owned(), "issued".to_owned()))
        );
    }
}
