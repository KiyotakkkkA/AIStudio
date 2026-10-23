use async_trait::async_trait;
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, FilePath};
use tokio::sync::oneshot;
use zvs_contract::{AppError, AppErrorCode, VectorSourceKind};
use zvs_host::platform::picker::PickerPort;

use crate::window;

pub const PICKER_CLOSED: &str = "Окно выбора файлов закрылось без ответа";

pub struct DialogPicker(AppHandle);

impl DialogPicker {
    pub const fn new(app: AppHandle) -> Self {
        Self(app)
    }
}

fn path_text(path: FilePath) -> Option<String> {
    path.into_path()
        .ok()
        .map(|path| path.to_string_lossy().into_owned())
        .filter(|path| !path.is_empty())
}

#[async_trait]
impl PickerPort for DialogPicker {
    async fn pick(&self, kind: VectorSourceKind) -> Result<Vec<String>, AppError> {
        let (sender, receiver) = oneshot::channel::<Vec<FilePath>>();
        let mut dialog = self.0.dialog().file();
        if let Some(studio) = self.0.get_window(window::STUDIO) {
            dialog = dialog.set_parent(&studio);
        }
        match kind {
            VectorSourceKind::Folder => dialog.pick_folder(move |path| {
                let _ = sender.send(path.into_iter().collect());
            }),
            VectorSourceKind::File => dialog.pick_files(move |paths| {
                let _ = sender.send(paths.unwrap_or_default());
            }),
        }
        let paths = receiver
            .await
            .map_err(|_| AppError::new(AppErrorCode::Unknown, PICKER_CLOSED))?;
        Ok(paths.into_iter().filter_map(path_text).collect())
    }

    async fn save_as(&self, suggested_name: &str) -> Result<Option<String>, AppError> {
        let (sender, receiver) = oneshot::channel::<Option<FilePath>>();
        let mut dialog = self.0.dialog().file().set_file_name(suggested_name);
        if let Some(studio) = self.0.get_window(window::STUDIO) {
            dialog = dialog.set_parent(&studio);
        }
        dialog.save_file(move |path| {
            let _ = sender.send(path);
        });
        let path = receiver
            .await
            .map_err(|_| AppError::new(AppErrorCode::Unknown, PICKER_CLOSED))?;
        Ok(path.and_then(path_text))
    }
}
