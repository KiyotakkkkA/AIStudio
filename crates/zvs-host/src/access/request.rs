use zvs_contract::{FileDiffDto, ProcessPlanDto, RunKind, ToolTier};

use crate::kernel::AdmissionOrigin;
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
            | Self::Desktop {
                sensitive: false, ..
            }
            | Self::Opaque { destructive: false } => Family::Benign,
            Self::Declared | Self::Remote { .. } | Self::Opaque { destructive: true } => {
                Family::Declared
            }
            Self::Browser
            | Self::Desktop {
                sensitive: true, ..
            } => Family::Browser,
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

    pub fn reviewed(&self) -> ReviewedAction {
        match self {
            Self::Plain
            | Self::Desktop {
                sensitive: false, ..
            }
            | Self::Opaque { destructive: false } => ReviewedAction::Plain,
            Self::Declared | Self::Remote { .. } | Self::Opaque { destructive: true } => {
                ReviewedAction::Declared
            }
            Self::Browser
            | Self::Desktop {
                sensitive: true, ..
            } => ReviewedAction::Browser,
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
    Folder(String),
    Site(String),
    App { exe: String, path: String },
    Service(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct AccessRequest {
    pub actor: Actor,
    pub tool: String,
    pub minimum: ToolTier,
    pub effect: Effect,
    pub resources: Vec<Resource>,
}

impl AccessRequest {
    pub fn new(actor: Actor, tool: impl Into<String>, effect: Effect) -> Self {
        Self {
            actor,
            tool: tool.into(),
            minimum: ToolTier::Auto,
            effect,
            resources: Vec::new(),
        }
    }

    pub fn review(run_id: impl Into<String>, action: &ReviewedAction) -> Self {
        let tool = match action {
            ReviewedAction::Process { tool, .. } => tool.clone(),
            _ => String::new(),
        };
        Self::new(Actor::run(run_id), tool, action.clone().into())
    }

    pub const fn at_least(mut self, minimum: ToolTier) -> Self {
        self.minimum = minimum;
        self
    }
}
