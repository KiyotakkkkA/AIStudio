use std::sync::Arc;

use zvs_contract::{
    AppError, AppErrorCode, BrowserBookmarkRenameInput, BrowserExtensionRef,
    BrowserExtensionToggleInput, BrowserExtensionsDto, BrowserHistoryQuery, BrowserImportInput,
    BrowserLibraryRef, VectorSourceKind,
};

use super::IpcServer;
use crate::services::Services;

pub const IMPORT_INTERRUPTED: &str = "Импорт прерван";
pub const EXTENSION_PICKER_UNAVAILABLE: &str = "Окно выбора папки недоступно";

async fn add_extension(services: Arc<Services>) -> Result<BrowserExtensionsDto, AppError> {
    let picker = services
        .picker
        .clone()
        .ok_or_else(|| AppError::new(AppErrorCode::NotImplemented, EXTENSION_PICKER_UNAVAILABLE))?;
    let Some(folder) = picker
        .pick(VectorSourceKind::Folder)
        .await?
        .into_iter()
        .next()
    else {
        return Ok(services.browser_extensions.list());
    };
    let extensions = services.browser_extensions.clone();
    tokio::task::spawn_blocking(move || extensions.add(std::path::Path::new(&folder)))
        .await
        .map_err(|_| AppError::new(AppErrorCode::Unknown, IMPORT_INTERRUPTED))?
}

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
            "browserExtensions.list",
            |services: Arc<Services>, _input: ()| async move {
                Ok::<_, AppError>(services.browser_extensions.list())
            },
        )
        .handle(
            "browserExtensions.add",
            |services: Arc<Services>, _input: ()| add_extension(services),
        )
        .handle(
            "browserExtensions.remove",
            |services: Arc<Services>, input: BrowserExtensionRef| async move {
                services.browser_extensions.remove(&input.id)
            },
        )
        .handle(
            "browserExtensions.setEnabled",
            |services: Arc<Services>, input: BrowserExtensionToggleInput| async move {
                services
                    .browser_extensions
                    .set_enabled(&input.id, input.enabled)
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
