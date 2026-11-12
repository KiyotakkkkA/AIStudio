pub mod admitted;
pub mod decision;
pub mod gate;
pub mod person;
pub mod pipeline;
pub mod request;
pub mod stages;

#[cfg(test)]
mod tests;

#[cfg(test)]
pub use admitted::admitted;
pub use admitted::{Admitted, Clearance};
pub use decision::{Decision, Outcome, StageName};
#[cfg(test)]
pub use gate::OpenGate;
pub use gate::{AccessGate, Gatekeeper, PERMISSION_REVOKED, SharedGate, review};
pub use person::{APPROVAL_DENIED, FOLDER_SUBJECT, grant, grant_approval, tool_approval};
pub use pipeline::{AccessPipeline, Pass, STAGE_FAILED, Stage, Step};
pub use request::{
    AccessRequest, Actor, Asking, Attendant, Cleared, Effect, Gating, GrantPrompt, Prompt, Resource,
};
pub use stages::{
    FloorStage, HardRulesStage, JudgeStage, ModeStage, OUTSIDE_ALLOWED_TOOLS, PERMISSION_DENIED,
    RulesStage, TOOL_DISABLED, TierStage,
};
