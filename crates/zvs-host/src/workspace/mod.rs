pub mod paths;
pub mod service;

#[cfg(test)]
mod tests;

pub use paths::{
    ForbiddenFolders, GrantTarget, RemovableMounts, TargetKind, canonical, covers, grant_target,
};
pub use service::{
    FOLDER_ACCESS_DENIED, FOLDER_FORBIDDEN, FOLDER_SUBJECT, FolderAccessService, FolderCall,
    FolderTarget, folder_access,
};
