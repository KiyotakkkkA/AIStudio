#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Observation {
    pub source: &'static str,
    pub observed_at: &'static str,
}

pub const ORIGIN: &str = "https://chat.z.ai";
pub const API_BASE: &str = "https://chat.z.ai/api";
pub const APP_PAGE: &str = "https://chat.z.ai/";

pub const MODELS_PATH: &str = "/models";
pub const CONFIG_PATH: &str = "/config";
pub const USER_PATH: &str = "/v1/auths/";
pub const CHATS_NEW_PATH: &str = "/v1/chats/new";
pub const COMPLETIONS_PATH: &str = "/v2/chat/completions";

pub const FE_VERSION: &str = "prod-fe-1.1.115";
pub const FE_VERSION_PATTERN: &str = r"/z-ai/frontend/(prod-fe-[0-9.]+)/";
pub const SIGNATURE_VERSION: &str = "0.0.1";
pub const PLATFORM: &str = "web";
pub const REGION: &str = "overseas";
pub const MESSAGE_VERSION: u32 = 1;

pub const DEFAULT_TIMEOUT_SECONDS: f64 = 600.0;

pub const MODELS_OBSERVATION: Observation = Observation {
    source: "https://chat.z.ai/api/models",
    observed_at: "2026-10-24",
};

pub const GENERATION_OBSERVATION: Observation = Observation {
    source: "https://chat.z.ai/api/v1/chats/new then POST /api/v2/chat/completions?… (SSE, X-FE-Version prod-fe-1.1.115)",
    observed_at: "2026-10-24",
};

pub fn chat_referer_path(chat_id: &str) -> String {
    format!("/c/{chat_id}")
}
