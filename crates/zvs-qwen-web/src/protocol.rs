/// Where and when a part of the vendor's wire format was observed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Observation {
    pub source: &'static str,
    pub observed_at: &'static str,
}

pub const ORIGIN: &str = "https://chat.qwen.ai";
pub const API_BASE: &str = "https://chat.qwen.ai/api/v2";
/// The application root the gateway loads; the vendor's own scripts run there.
pub const APP_PAGE: &str = "https://chat.qwen.ai/";

pub const MODELS_PATH: &str = "/models/";
pub const CHATS_NEW_PATH: &str = "/chats/new";
pub const COMPLETIONS_PATH: &str = "/chat/completions";
pub const SESSION_REFRESH_URL: &str = "https://auth.qwen.ai/api/v2/auths/refresh";
pub const REQUEST_ORIGIN: &str = "chat.qwen.ai";

pub const MODELS_REFERER_PATH: &str = "/";
pub const NEW_CHAT_REFERER_PATH: &str = "/c/new-chat";

pub const CHAT_MODE: &str = "normal";
pub const CHAT_TYPE: &str = "t2t";
pub const SEARCH_CHAT_TYPE: &str = "search";
pub const EXCLUDED_POLICY_CHECK_TOOLS_NAMES: &[&str] = &["web_search"];
pub const API_VERSION: &str = "2.1";
/// The frontend build the client announces in its `Version` header.
pub const WEB_VERSION: &str = "0.3.22";
pub const WEB_VERSION_PATTERN: &str = r"/qwen-chat-fe/(\d+\.\d+\.\d+)/";
pub const SOURCE: &str = "web";

/// Thinking turns can run for minutes before the first answer token.
pub const DEFAULT_TIMEOUT_SECONDS: f64 = 600.0;

pub const MODELS_OBSERVATION: Observation = Observation {
    source: "https://chat.qwen.ai/api/v2/models/",
    observed_at: "2026-09-10",
};

pub const GENERATION_OBSERVATION: Observation = Observation {
    source: "https://chat.qwen.ai/api/v2/chats/new then POST /api/v2/chat/completions?chat_id=… (SSE, FE Version 0.3.22, turns chained by parent_id, chat_type t2t or search)",
    observed_at: "2026-10-26",
};

pub fn chat_referer_path(chat_id: &str) -> String {
    format!("/c/{chat_id}")
}
