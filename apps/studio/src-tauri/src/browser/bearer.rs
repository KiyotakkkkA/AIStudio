use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use url::Url;
use zvs_host::drivers::ai::AccountToken;

pub const ACCOUNT_ORIGINS: [&str; 2] = ["https://chat.qwen.ai", "https://chat.z.ai"];
const FIXTURE_HOST: &str = "127.0.0.1";
const BEARER: &str = "bearer";

#[cfg(any(test, feature = "e2e"))]
fn fixture_origin(url: &Url) -> bool {
    url.scheme() == "http"
        && url.host_str() == Some(FIXTURE_HOST)
        && url.port().is_some()
        && url.username().is_empty()
        && url.password().is_none()
}

#[cfg(not(any(test, feature = "e2e")))]
const fn fixture_origin(_url: &Url) -> bool {
    false
}

pub fn origin_of(url: &str) -> Option<String> {
    let url = Url::parse(url).ok()?;
    matches!(url.scheme(), "http" | "https").then(|| url.origin().ascii_serialization())
}

pub fn account_origin(url: &str) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    let origin = origin_of(url)?;
    (ACCOUNT_ORIGINS.contains(&origin.as_str()) || fixture_origin(&parsed)).then_some(origin)
}

pub fn request_filters() -> Vec<String> {
    let mut filters = ACCOUNT_ORIGINS.map(str::to_owned).to_vec();
    if cfg!(any(test, feature = "e2e")) {
        filters.push(format!("http://{FIXTURE_HOST}:*"));
    }
    filters
}

#[derive(Clone, PartialEq, Eq)]
pub struct Sighting {
    pub request_url: String,
    pub page_url: String,
    pub authorization: Option<String>,
}

impl fmt::Debug for Sighting {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Sighting")
            .field("request_url", &self.request_url)
            .field("page_url", &self.page_url)
            .field("authorization", &self.authorization.is_some())
            .finish()
    }
}

fn bearer_token(header: &str) -> Option<&str> {
    let scheme = header.get(..BEARER.len())?;
    if !scheme.eq_ignore_ascii_case(BEARER) {
        return None;
    }
    let rest = &header[BEARER.len()..];
    let token = rest.trim_start();
    let valid =
        token.len() < rest.len() && !token.is_empty() && !token.chars().any(char::is_whitespace);
    valid.then_some(token)
}

pub fn captured(sighting: &Sighting) -> Option<(String, AccountToken)> {
    let origin = account_origin(&sighting.request_url)?;
    if origin_of(&sighting.page_url)? != origin {
        return None;
    }
    let token = bearer_token(sighting.authorization.as_deref()?)?;
    Some((origin, AccountToken::bearer(token)))
}

#[derive(Clone, Default)]
pub struct BearerTokens(Arc<Mutex<HashMap<String, AccountToken>>>);

impl BearerTokens {
    fn lock(&self) -> MutexGuard<'_, HashMap<String, AccountToken>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn observe(&self, sighting: &Sighting) -> bool {
        let Some((origin, token)) = captured(sighting) else {
            return false;
        };
        self.lock().insert(origin, token);
        true
    }

    pub fn token(&self, url: &str) -> Option<AccountToken> {
        let origin = origin_of(url)?;
        self.lock().get(&origin).cloned()
    }
}

impl fmt::Debug for BearerTokens {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BearerTokens")
            .field("origins", &self.lock().keys().cloned().collect::<Vec<_>>())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const QWEN: &str = "https://chat.qwen.ai/api/v1/auths/";

    fn seen(request_url: &str, authorization: &str, page_url: &str) -> Sighting {
        Sighting {
            request_url: request_url.to_owned(),
            page_url: page_url.to_owned(),
            authorization: Some(authorization.to_owned()),
        }
    }

    #[test]
    fn tokens_stay_scoped_to_the_vendor_page_that_sent_them() {
        let tokens = BearerTokens::default();
        assert!(tokens.observe(&seen(QWEN, "Bearer original", QWEN)));
        assert!(!tokens.observe(&seen(QWEN, "Bearer hostile", "https://untrusted.example/")));
        assert!(!tokens.observe(&seen(QWEN, "Basic unsupported", QWEN)));
        assert!(!tokens.observe(&seen(
            "https://untrusted.example/api",
            "Bearer unrelated",
            "https://untrusted.example/"
        )));
        assert!(!tokens.observe(&Sighting {
            authorization: None,
            ..seen(QWEN, "", QWEN)
        }));
        assert_eq!(tokens.token(QWEN), Some(AccountToken::bearer("original")));
        assert_eq!(tokens.token("https://untrusted.example/"), None);
        assert_eq!(tokens.token("not a url"), None);
        assert!(!format!("{tokens:?}").contains("original"));
    }

    #[test]
    fn a_third_party_frame_on_another_origin_cannot_set_the_token() {
        let tokens = BearerTokens::default();
        assert!(!tokens.observe(&seen(
            QWEN,
            "Bearer framed",
            "https://ads.example/frame.html"
        )));
        assert!(!tokens.observe(&seen(QWEN, "Bearer cross", "https://chat.qwen.ai.example/")));
        assert_eq!(tokens.token(QWEN), None);
        assert!(tokens.observe(&seen(QWEN, "Bearer own", "https://chat.qwen.ai/c/chat")));
        assert_eq!(tokens.token(QWEN), Some(AccountToken::bearer("own")));
    }

    #[test]
    fn the_bearer_header_follows_the_ts_pattern() {
        assert_eq!(bearer_token("Bearer abc"), Some("abc"));
        assert_eq!(bearer_token("bearer\tabc"), Some("abc"));
        assert_eq!(bearer_token("BEARER   abc.def-ghi"), Some("abc.def-ghi"));
        assert_eq!(bearer_token("Bearerabc"), None);
        assert_eq!(bearer_token("Bearer "), None);
        assert_eq!(bearer_token("Bearer a b"), None);
        assert_eq!(bearer_token("Bearer abc "), None);
        assert_eq!(bearer_token("Basic abc"), None);
        assert_eq!(bearer_token("Bear"), None);
        assert_eq!(bearer_token("Беарер abc"), None);
    }

    #[test]
    fn the_allow_list_is_exact_and_the_fixture_origin_exists_only_in_tests() {
        assert_eq!(
            account_origin(QWEN).as_deref(),
            Some("https://chat.qwen.ai")
        );
        assert_eq!(
            account_origin("https://chat.z.ai/api/v1/auths/").as_deref(),
            Some("https://chat.z.ai")
        );
        for refused in [
            "http://chat.z.ai/",
            "https://chat.z.ai.evil.example/",
            "https://z.ai/",
            "http://chat.qwen.ai/",
            "https://chat.qwen.ai.evil.example/",
            "https://evil.chat.qwen.ai/",
            "https://chat.qwen.ai:8443/",
            "file:///C:/",
            "http://127.0.0.1/",
            "http://user:pass@127.0.0.1:9/",
        ] {
            assert_eq!(account_origin(refused), None, "{refused}");
        }
        assert_eq!(
            account_origin("http://127.0.0.1:4100/robots.txt").as_deref(),
            Some("http://127.0.0.1:4100")
        );
        assert_eq!(request_filters().len(), 3);
    }
}
