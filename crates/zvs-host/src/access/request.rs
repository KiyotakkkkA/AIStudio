use std::fmt;
use std::sync::Arc;

use serde_json::Value;
use zvs_contract::{AppError, ApprovalRequestDto, FileDiffDto, ProcessPlanDto, RunKind, ToolTier};

use crate::computer::policy::terminal;
use crate::data::schema::ApprovalDecision;
use crate::kernel::{AdmissionOrigin, PermissionRequirement};
use crate::policy::{Family, FileAction, ReviewedAction};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Actor {
    pub run_id: String,
    pub kind: Option<RunKind>,
    pub origin: AdmissionOrigin,
    pub listed: bool,
    pub scopes: Vec<String>,
}

impl Actor {
    pub fn run(run_id: impl Into<String>) -> Self {
        Self {
            run_id: run_id.into(),
            listed: true,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    Plain,
    Read,
    Declared,
    Browser,
    Files {
        actions: Vec<FileAction>,
        changes: Vec<FileDiffDto>,
    },
    Process {
        tool: String,
        process: ProcessPlanDto,
    },
    Desktop {
        apps: Vec<String>,
        sensitive: bool,
    },
    Remote {
        service: String,
        action: String,
    },
    Opaque {
        destructive: bool,
    },
}

impl Effect {
    pub fn family(&self) -> Family {
        match self {
            Self::Plain
            | Self::Read
            | Self::Desktop {
                sensitive: false, ..
            }
            | Self::Opaque { destructive: false } => Family::Benign,
            Self::Declared | Self::Remote { .. } | Self::Opaque { destructive: true } => {
                Family::Declared
            }
            Self::Desktop { apps, .. } if apps.iter().any(|app| terminal(app)) => Family::Process,
            Self::Browser | Self::Desktop { .. } => Family::Browser,
            Self::Files { actions, .. } => {
                if actions.iter().any(|action| action.operation.destructive()) {
                    Family::Files
                } else {
                    Family::Benign
                }
            }
            Self::Process { .. } => Family::Process,
        }
    }

    pub fn reviewable(&self) -> bool {
        !matches!(self, Self::Desktop { sensitive: true, apps } if apps.iter().any(|app| terminal(app)))
    }

    pub fn benign(&self) -> bool {
        self.family() == Family::Benign
    }

    pub fn reviewed(&self) -> ReviewedAction {
        match self {
            Self::Plain
            | Self::Read
            | Self::Desktop {
                sensitive: false, ..
            }
            | Self::Opaque { destructive: false } => ReviewedAction::Plain,
            Self::Declared | Self::Remote { .. } | Self::Opaque { destructive: true } => {
                ReviewedAction::Declared
            }
            Self::Browser | Self::Desktop { .. } => ReviewedAction::Browser,
            Self::Files { actions, changes } => ReviewedAction::Files {
                actions: actions.clone(),
                changes: changes.clone(),
            },
            Self::Process { tool, process } => ReviewedAction::Process {
                tool: tool.clone(),
                process: process.clone(),
            },
        }
    }
}

impl From<ReviewedAction> for Effect {
    fn from(action: ReviewedAction) -> Self {
        match action {
            ReviewedAction::Plain => Self::Plain,
            ReviewedAction::Declared => Self::Declared,
            ReviewedAction::Browser => Self::Browser,
            ReviewedAction::Files { actions, changes } => Self::Files { actions, changes },
            ReviewedAction::Process { tool, process } => Self::Process { tool, process },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resource {
    Folder {
        path: String,
        creates: Vec<String>,
    },
    Site(String),
    App {
        exe: String,
        path: String,
        icon: Option<String>,
    },
    Service(String),
}

impl Resource {
    pub const fn granted_by_prompt(&self) -> bool {
        matches!(self, Self::Folder { .. } | Self::App { .. })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Gating {
    Open,
    #[default]
    Tool,
    AppFolder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cleared {
    Admission,
    Review,
    Person,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asking {
    Grant,
    Tool,
}

pub trait Attendant: Send + Sync {
    fn cleared(&self, _by: Cleared) {}

    fn asking(&self, _asking: Asking, _approval: &ApprovalRequestDto) {}

    fn answered(
        &self,
        _asking: Asking,
        _approval: &ApprovalRequestDto,
        decision: Result<ApprovalDecision, AppError>,
    ) -> Result<ApprovalDecision, AppError> {
        decision
    }
}

#[derive(Clone, Default)]
pub struct Prompt {
    pub kind: Option<String>,
    pub conversation: Option<String>,
    pub action: Option<String>,
    pub scope: Option<String>,
    pub fields: Vec<(String, Value)>,
    pub denied: Option<String>,
    pub grant: Option<GrantPrompt>,
    pub attendant: Option<Arc<dyn Attendant>>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct GrantPrompt {
    pub tool: Option<String>,
    pub action: Option<String>,
    pub scope: Option<String>,
    pub detail: Option<String>,
}

impl fmt::Debug for Prompt {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Prompt")
            .field("kind", &self.kind)
            .field("conversation", &self.conversation)
            .field("action", &self.action)
            .field("scope", &self.scope)
            .field("fields", &self.fields)
            .field("denied", &self.denied)
            .field("grant", &self.grant)
            .field("attendant", &self.attendant.is_some())
            .finish()
    }
}

impl PartialEq for Prompt {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
            && self.conversation == other.conversation
            && self.action == other.action
            && self.scope == other.scope
            && self.fields == other.fields
            && self.denied == other.denied
            && self.grant == other.grant
            && self.attendant.is_some() == other.attendant.is_some()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AccessRequest {
    pub actor: Actor,
    pub tool: String,
    pub gating: Gating,
    pub minimum: ToolTier,
    pub effect: Effect,
    pub resources: Vec<Resource>,
    pub always_ask: bool,
    pub prompt: Prompt,
}

impl AccessRequest {
    pub fn new(actor: Actor, tool: impl Into<String>, effect: Effect) -> Self {
        Self {
            actor,
            tool: tool.into(),
            gating: Gating::Tool,
            minimum: ToolTier::Auto,
            effect,
            resources: Vec::new(),
            always_ask: false,
            prompt: Prompt::default(),
        }
    }

    pub fn open() -> Self {
        Self {
            gating: Gating::Open,
            ..Self::new(Actor::default(), String::new(), Effect::Plain)
        }
    }

    pub fn grants(tool: impl Into<String>, resources: Vec<Resource>) -> Self {
        Self {
            gating: Gating::Open,
            resources,
            ..Self::new(Actor::default(), tool, Effect::Plain)
        }
    }

    pub fn of(requirement: &PermissionRequirement) -> Self {
        match requirement {
            PermissionRequirement::None | PermissionRequirement::Kernel => Self::open(),
            PermissionRequirement::Tool { tool, tier } => {
                let minimum = tier.unwrap_or(ToolTier::Auto);
                let effect = if minimum == ToolTier::Ask {
                    Effect::Declared
                } else {
                    Effect::Plain
                };
                Self::new(Actor::default(), tool.clone(), effect).at_least(minimum)
            }
            PermissionRequirement::AppFolder { tool } => Self {
                gating: Gating::AppFolder,
                ..Self::new(Actor::default(), tool.clone(), Effect::Plain)
            },
        }
    }

    pub fn review(run_id: impl Into<String>, effect: &Effect) -> Self {
        let tool = match effect {
            Effect::Process { tool, .. } => tool.clone(),
            _ => String::new(),
        };
        Self::new(Actor::run(run_id), tool, effect.clone())
    }

    pub const fn at_least(mut self, minimum: ToolTier) -> Self {
        self.minimum = minimum;
        self
    }

    #[must_use]
    pub fn with_effect(mut self, effect: Effect) -> Self {
        self.effect = effect;
        self
    }

    #[must_use]
    pub fn raised(mut self) -> Self {
        match self.gating {
            Gating::Open => {}
            Gating::AppFolder => {
                self.gating = Gating::Tool;
                self.minimum = ToolTier::Ask;
            }
            Gating::Tool if self.minimum == ToolTier::Off => {}
            Gating::Tool => self.minimum = ToolTier::Ask,
        }
        if self.gating == Gating::Tool && self.effect.benign() {
            self.effect = Effect::Declared;
        }
        self
    }

    pub fn grant_resources(&self) -> Vec<&Resource> {
        self.resources
            .iter()
            .filter(|resource| resource.granted_by_prompt())
            .collect()
    }
}
