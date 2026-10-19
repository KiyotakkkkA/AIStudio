//! How the gateway page sends a request so that it leaves the browser the way the client's does.

use crate::protocol::{API_BASE, CHATS_NEW_PATH};

/// The page API a request goes through. The vendor's anti-abuse SDK instruments the page's
/// XHR, and the client creates chats over XHR; streamed completions use `fetch`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageTransport {
    Fetch,
    Xhr,
}

impl PageTransport {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fetch => "fetch",
            Self::Xhr => "xhr",
        }
    }
}

/// Case-insensitive pattern over the `id`, `class` and `src` of elements the page adds when
/// the vendor asks a person to verify (Alibaba's slider dialog and punish frame).
pub const CHALLENGE_PATTERN: &str = "baxia|punish|nocaptcha|_____tmd_____";

/// The `localStorage` key under which the client keeps its current bearer. The client renews
/// it through its own session refresh (`/api/v2/auths/refresh`, cookie `refresh_token`) and
/// sends `Authorization: Bearer <it>`; a token minted elsewhere can already be revoked.
/// Observed 2026-10-19.
pub const PAGE_BEARER_KEY: &str = "token";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PagePlan {
    pub transport: PageTransport,
    /// Watch the page for a verification dialog while the request waits for its response.
    pub challenge_pattern: Option<&'static str>,
    /// Replace a request's bearer with the one the client keeps under this `localStorage`
    /// key. A request sent without `Authorization` stays on the cookie session.
    pub page_bearer_key: Option<&'static str>,
}

/// The plan for `url`, an absolute URL on the Qwen origin.
pub fn page_plan(url: &str) -> PagePlan {
    let path = url
        .strip_prefix(API_BASE)
        .map(|rest| rest.split(['?', '#']).next().unwrap_or_default());
    PagePlan {
        transport: if path == Some(CHATS_NEW_PATH) {
            PageTransport::Xhr
        } else {
            PageTransport::Fetch
        },
        challenge_pattern: Some(CHALLENGE_PATTERN),
        page_bearer_key: Some(PAGE_BEARER_KEY),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chats_are_created_over_xhr_and_everything_else_over_fetch() {
        let plan = |url: &str| page_plan(url).transport;
        assert_eq!(
            plan("https://chat.qwen.ai/api/v2/chats/new"),
            PageTransport::Xhr
        );
        assert_eq!(
            plan("https://chat.qwen.ai/api/v2/chats/new?x=1"),
            PageTransport::Xhr
        );
        assert_eq!(
            plan("https://chat.qwen.ai/api/v2/chat/completions?chat_id=a"),
            PageTransport::Fetch
        );
        assert_eq!(
            plan("https://chat.qwen.ai/api/v2/models/"),
            PageTransport::Fetch
        );
        assert_eq!(
            plan("https://chat.qwen.ai/api/v2/chats/newer"),
            PageTransport::Fetch
        );
        assert_eq!(PageTransport::Xhr.as_str(), "xhr");
        assert!(page_plan(API_BASE).challenge_pattern.is_some());
        assert_eq!(page_plan(API_BASE).page_bearer_key, Some("token"));
    }
}
