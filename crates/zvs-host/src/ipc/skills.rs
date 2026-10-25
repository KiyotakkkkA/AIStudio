use std::sync::Arc;

use zvs_contract::{
    AppError, CreateSkillInput, SetSkillEnabledInput, SkillExportInput, SkillFileRef,
    SkillImportInput, SkillRef, SkillRemoveOutput, UpdateSkillInput,
};

use super::IpcServer;
use crate::services::Services;

pub fn register(server: &mut IpcServer) {
    server
        .handle(
            "skills.list",
            |services: Arc<Services>, _input: ()| async move { services.skills.list() },
        )
        .handle(
            "skills.get",
            |services: Arc<Services>, input: SkillRef| async move { services.skills.get(&input.id) },
        )
        .handle(
            "skills.create",
            |services: Arc<Services>, input: CreateSkillInput| async move {
                services.skills.create(&input)
            },
        )
        .handle(
            "skills.update",
            |services: Arc<Services>, input: UpdateSkillInput| async move {
                services.skills.update(&input)
            },
        )
        .handle(
            "skills.remove",
            |services: Arc<Services>, input: SkillRef| async move {
                services.skills.remove(&input.id)?;
                Ok::<SkillRemoveOutput, AppError>(SkillRemoveOutput {
                    id: input.id,
                    removed: true,
                })
            },
        )
        .handle(
            "skills.setEnabled",
            |services: Arc<Services>, input: SetSkillEnabledInput| async move {
                services.skills.set_enabled(&input.id, input.enabled)
            },
        )
        .handle(
            "skills.duplicate",
            |services: Arc<Services>, input: SkillRef| async move {
                services.skills.duplicate(&input.id)
            },
        )
        .handle(
            "skills.updateBundled",
            |services: Arc<Services>, input: SkillRef| async move {
                services.skills.update_bundled(&input.id)
            },
        )
        .handle(
            "skills.updateAllBundled",
            |services: Arc<Services>, _input: ()| async move {
                services.skills.update_all_bundled()
            },
        )
        .handle(
            "skills.readFile",
            |services: Arc<Services>, input: SkillFileRef| async move {
                services.skills.read_file(&input.id, &input.path)
            },
        )
        .handle(
            "skills.import",
            |services: Arc<Services>, input: SkillImportInput| async move {
                services.skills.import(&input).await
            },
        )
        .handle(
            "skills.export",
            |services: Arc<Services>, input: SkillExportInput| async move {
                services.skills.export(&input).await
            },
        );
}

#[cfg(test)]
mod tests;
