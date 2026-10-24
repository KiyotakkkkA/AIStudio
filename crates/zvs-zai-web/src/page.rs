use crate::protocol::{API_BASE, CHATS_NEW_PATH, COMPLETIONS_PATH};

pub const PAGE_BEARER_KEY: &str = "token";
pub const BEARER_QUERY: &str = "token";
pub const FE_VERSION_HEADER: &str = "x-fe-version";
pub const CHALLENGE_PATTERN: &str = "aliyunCaptcha";
pub const CAPTCHA_FIELD: &str = "captcha_verify_param";
pub const CAPTCHA_REQUIRED_CODE: &str = "FRONTEND_CAPTCHA_REQUIRED";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Captcha {
    pub script_url: &'static str,
    pub region: &'static str,
    pub prefix: &'static str,
    pub scene_id: &'static str,
    pub mode: &'static str,
    pub element_id: &'static str,
    pub button_id: &'static str,
    pub timeout_ms: u64,
    pub field: &'static str,
}

pub const CAPTCHA: Captcha = Captcha {
    script_url: "https://o.alicdn.com/captcha-frontend/aliyunCaptcha/AliyunCaptcha.js",
    region: "sgp",
    prefix: "no8xfe",
    scene_id: "didk33e0",
    mode: "popup",
    element_id: "zvs-captcha-element",
    button_id: "zvs-captcha-trigger",
    timeout_ms: 10_000,
    field: CAPTCHA_FIELD,
};

fn api_path(url: &str) -> Option<&str> {
    url.strip_prefix(API_BASE)
        .map(|rest| rest.split(['?', '#']).next().unwrap_or_default())
}

pub fn is_turn(url: &str) -> bool {
    matches!(api_path(url), Some(CHATS_NEW_PATH | COMPLETIONS_PATH))
}

pub fn needs_captcha(url: &str) -> bool {
    api_path(url) == Some(COMPLETIONS_PATH)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_completions_need_a_captcha_and_both_turn_requests_are_turns() {
        for (url, turn, captcha) in [
            ("https://chat.z.ai/api/v1/chats/new", true, false),
            (
                "https://chat.z.ai/api/v2/chat/completions?timestamp=1",
                true,
                true,
            ),
            ("https://chat.z.ai/api/models", false, false),
            ("https://chat.z.ai/api/v1/auths/", false, false),
            ("https://chat.z.ai/api/v2/chat/completionsx", false, false),
        ] {
            assert_eq!(is_turn(url), turn, "{url}");
            assert_eq!(needs_captcha(url), captcha, "{url}");
        }
    }

    #[test]
    fn the_captcha_is_the_one_the_chat_z_ai_host_loads_in_elements_of_its_own() {
        assert_eq!(CAPTCHA.scene_id, "didk33e0");
        assert!(CAPTCHA.script_url.starts_with("https://o.alicdn.com/"));
        assert!(!CAPTCHA.element_id.contains(CHALLENGE_PATTERN));
        assert_ne!(CAPTCHA.element_id, "chat-captcha-element");
        assert_eq!(CAPTCHA.field, "captcha_verify_param");
    }
}
