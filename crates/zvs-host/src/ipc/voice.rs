use std::sync::Arc;

use zvs_contract::{AppError, VoiceModelInput, VoiceSelectInput, VoiceSelectModelInput};

use super::IpcServer;
use crate::services::Services;

pub fn register(server: &mut IpcServer) {
    server
        .handle(
            "voice.devices",
            |services: Arc<Services>, (): ()| async move { services.voice.devices().await },
        )
        .handle(
            "voice.select",
            |services: Arc<Services>, input: VoiceSelectInput| async move {
                services.voice.select_device(&input).await
            },
        )
        .handle(
            "voice.models",
            |services: Arc<Services>, (): ()| async move { services.voice.models().await },
        )
        .handle(
            "voice.installModel",
            |services: Arc<Services>, input: VoiceModelInput| async move {
                services.voice.install_model(&input).await
            },
        )
        .handle(
            "voice.selectModel",
            |services: Arc<Services>, input: VoiceSelectModelInput| async move {
                services.voice.select_model(&input).await
            },
        )
        .handle(
            "voice.startRecording",
            |services: Arc<Services>, (): ()| async move { services.voice.start_recording().await },
        )
        .handle(
            "voice.stopRecording",
            |services: Arc<Services>, (): ()| async move { services.voice.stop_recording().await },
        )
        .handle(
            "voice.cancelRecording",
            |services: Arc<Services>, (): ()| async move {
                services.voice.cancel_recording().await?;
                Ok::<_, AppError>(())
            },
        );
}

#[cfg(test)]
mod tests;
