pub mod paths;
pub mod service;

#[cfg(test)]
mod tests;

pub use paths::{
    ForbiddenFolders, GrantTarget, RemovableMounts, TargetKind, canonical, covers, grant_target,
};
pub use service::{
    AccessPlan, CHANGES_DENIED, FOLDER_ACCESS_DENIED, FOLDER_FORBIDDEN, FOLDER_SUBJECT,
    FolderAccessService, FolderCall, FolderTarget, Review, folder_access,
};
