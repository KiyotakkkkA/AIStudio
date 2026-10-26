//! The Qwen web client's protocol as observed on chat.qwen.ai, kept apart from any transport.
//!
//! The crate builds request bodies, decodes the model list and the streamed turn, and records
//! the date of every observation it relies on. It sends nothing itself: the host supplies the
//! browser session, and that session is what the vendor's own page uses.

pub mod models;
pub mod page;
pub mod protocol;
pub mod request;
pub mod stream;

pub use models::{ModelList, ModelsDiagnostics, QwenModel, RowProblem, parse_models};
pub use page::{
    CHALLENGE_PATTERN, FRONTEND_VERSION, FrontendVersion, PAGE_BEARER_KEY, PagePlan, PageTransport,
    SESSION_REFRESH, SESSION_REFUSALS, SessionRefresh, page_plan,
};
pub use protocol::Observation;
pub use request::{ChatType, CompletionTurn, ThinkingMode, new_chat_body, parse_chat_created};
pub use stream::{TurnDecoder, TurnEvent, Usage, VendorFailure, WebSource};
