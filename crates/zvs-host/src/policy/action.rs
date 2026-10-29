use serde::{Deserialize, Serialize};
use zvs_contract::{FileChangeKind, FileDiffDto, ProcessPlanDto};

use super::table::Family;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FileOperation {
    Create,
    Overwrite,
    Move,
    MoveOnto,
    Delete,
}

impl FileOperation {
    pub const fn destructive(self) -> bool {
        matches!(self, Self::Overwrite | Self::MoveOnto | Self::Delete)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileAction {
    pub operation: FileOperation,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ReviewedAction {
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewVerdict {
    Run(zvs_contract::ToolAuthorisationDto),
    Ask { reason: Option<String> },
    Deny { rule: String },
}

impl ReviewedAction {
    pub fn files(changes: Vec<FileDiffDto>, existed: impl Fn(&str) -> bool) -> Self {
        let actions = changes
            .iter()
            .map(|change| FileAction {
                operation: match change.kind {
                    FileChangeKind::Create => FileOperation::Create,
                    FileChangeKind::Modify => FileOperation::Overwrite,
                    FileChangeKind::Delete => FileOperation::Delete,
                    FileChangeKind::Move if existed(&change.path) => FileOperation::MoveOnto,
                    FileChangeKind::Move => FileOperation::Move,
                },
                path: change.path.clone(),
                from: change.from.clone(),
            })
            .collect();
        Self::Files { actions, changes }
    }

    pub fn family(&self) -> Family {
        match self {
            Self::Plain => Family::Benign,
            Self::Declared => Family::Declared,
            Self::Browser => Family::Browser,
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
}
