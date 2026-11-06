use std::sync::Arc;

use zvs_contract::{
    AppError, AppErrorCode, BrowserBookmarkRenameInput, BrowserHistoryQuery, BrowserImportInput,
    BrowserLibraryRef,
};

use super::IpcServer;
use crate::services::Services;

pub const IMPORT_INTERRUPTED: &str = "Импорт прерван";

pub fn register(server: &mut IpcServer) {
    server
        .handle(
            "browserHistory.list",
            |services: Arc<Services>, input: BrowserHistoryQuery| async move {
                services.browser_library.history(input.query.as_deref())
            },
        )
        .handle(
            "browserHistory.remove",
            |services: Arc<Services>, input: BrowserLibraryRef| async move {
                services.browser_library.remove_visit(&input.id)
            },
        )
        .handle(
            "browserHistory.clear",
            |services: Arc<Services>, _input: ()| async move {
                services.browser_library.clear_history()
            },
        )
        .handle(
            "browserBookmarks.list",
            |services: Arc<Services>, _input: ()| async move { services.browser_library.bookmarks() },
        )
        .handle(
            "browserBookmarks.rename",
            |services: Arc<Services>, input: BrowserBookmarkRenameInput| async move {
                services.browser_library.rename(&input.id, &input.title)
            },
        )
        .handle(
            "browserBookmarks.remove",
            |services: Arc<Services>, input: BrowserLibraryRef| async move {
                services.browser_library.remove_bookmark(&input.id)
            },
        )
        .handle(
            "browserImport.sources",
            |services: Arc<Services>, _input: ()| async move {
                let importer = services.browser_importer.clone();
                tokio::task::spawn_blocking(move || importer.sources())
                    .await
                    .map_err(|_| AppError::new(AppErrorCode::Unknown, IMPORT_INTERRUPTED))
            },
        )
        .handle(
            "browserImport.run",
            |services: Arc<Services>, input: BrowserImportInput| async move {
                let importer = services.browser_importer.clone();
                tokio::task::spawn_blocking(move || importer.run(&input))
                    .await
                    .map_err(|_| AppError::new(AppErrorCode::Unknown, IMPORT_INTERRUPTED))?
            },
        );
}
