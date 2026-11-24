use std::path::Path;
use std::sync::Arc;

use super::rewrite::{PathRewrite, rewrite_database};
use super::shared::{initialize_locked, io};
use super::{BootError, StateRootLock, StepContext, StepProgress};
use crate::data::{Repositories, repositories::SecretPatch};
use crate::platform::paths::{PathEnvironment, StudioPaths};
use crate::services::crypto::{CryptoService, os_backend};
use crate::services::shared_credentials::{CredentialReference, SharedCredentials};

fn adopt_openai(
    connection: &rusqlite::Connection,
    paths: &crate::platform::paths::SharedPaths,
) -> Result<(), BootError> {
    let has_accounts = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name = 'account')",
            [],
            |row| row.get::<_, bool>(0),
        )
        .map_err(io)?;
    if !has_accounts {
        return Ok(());
    }
    let repositories = Repositories::new(connection);
    let accounts = repositories
        .accounts
        .list_by_adapter(zvs_contract::AccountFamily::Openai)
        .map_err(io)?;
    if accounts.is_empty() {
        return Ok(());
    }
    let credentials = SharedCredentials::new(paths, Arc::new(CryptoService::new(os_backend())));
    for account in accounts {
        let Some(id) = &account.token_secret_id else {
            continue;
        };
        if let Some(cipher) = repositories
            .secrets
            .find_cipher_by_id(id)
            .map_err(io)?
            .and_then(|row| row.cipher)
        {
            credentials
                .adopt(CredentialReference::openai(&account).map_err(io)?, &cipher)
                .map_err(io)?;
            repositories
                .secrets
                .update(
                    id,
                    &SecretPatch {
                        cipher: Some(None),
                        hint: Some(None),
                        ..SecretPatch::default()
                    },
                )
                .map_err(io)?;
        }
    }
    Ok(())
}

fn move_skills(from: &Path, to: &Path) -> Result<(), BootError> {
    let entries = match std::fs::read_dir(from) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(io(error)),
    };
    for entry in entries {
        let entry = entry.map_err(io)?;
        let target = to.join(entry.file_name());
        match std::fs::symlink_metadata(&target) {
            Ok(_) => return Err(BootError::DestinationConflict(target)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(io(error)),
        }
        std::fs::rename(entry.path(), target).map_err(io)?;
    }
    Ok(())
}

pub(super) fn upgrade(
    context: &StepContext<'_>,
    progress: &dyn Fn(StepProgress),
) -> Result<(), BootError> {
    let Some(layout) = context.layout else {
        return Ok(());
    };
    let _lock = StateRootLock::blocking_exclusive(&layout.shared.root).map_err(io)?;
    progress(StepProgress { done: 0, total: 3 });
    initialize_locked(layout)?;
    if context.root != layout.desktop {
        match std::fs::remove_dir(&layout.desktop) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) if error.kind() == std::io::ErrorKind::DirectoryNotEmpty => {
                return Err(BootError::DestinationConflict(layout.desktop.clone()));
            }
            Err(error) => return Err(io(error)),
        }
        std::fs::rename(context.root, &layout.desktop).map_err(io)?;
    }
    progress(StepProgress { done: 1, total: 3 });
    let paths_at = |root: &Path| {
        StudioPaths::resolve(&PathEnvironment {
            state_dir: root.to_path_buf(),
            resource_dir: Default::default(),
            executable_dir: Default::default(),
            packaged: false,
        })
    };
    let unshared = paths_at(&layout.desktop);
    let source = paths_at(
        layout
            .previous_desktop
            .as_deref()
            .unwrap_or(&layout.desktop),
    );
    move_skills(&unshared.skills_dir, &layout.shared.skills)?;
    progress(StepProgress { done: 2, total: 3 });
    let target = unshared.clone().with_shared(&layout.shared);
    if target.db_path.try_exists().map_err(io)? {
        let mut connection = rusqlite::Connection::open(&target.db_path).map_err(io)?;
        let transaction = connection.transaction().map_err(io)?;
        adopt_openai(&transaction, &layout.shared)?;
        if transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name = 'skill')",
                [],
                |row| row.get::<_, bool>(0),
            )
            .map_err(io)?
        {
            transaction
                .execute(
                    "UPDATE skill SET path = 'skills:' || substr(path, length('data:library/skills/') + 1)
                     WHERE instr(path, 'data:library/skills/') = 1",
                    [],
                )
                .map_err(io)?;
        }
        let mut rewrites = vec![PathRewrite::new(
            unshared.skills_dir,
            target.skills_dir.clone(),
        )];
        if source.root != target.root {
            rewrites.extend([
                PathRewrite::new(source.skills_dir, target.skills_dir.clone()),
                PathRewrite::new(source.root.clone(), target.root.clone()),
            ]);
        }
        rewrite_database(
            &transaction,
            &source.root,
            &rewrites,
            &target.stored_paths(),
        )
        .map_err(io)?;
        transaction.commit().map_err(io)?;
    }
    progress(StepProgress { done: 3, total: 3 });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boot::{Boot, BootDecision};
    use crate::platform::paths::StudioLayout;
    use zvs_contract::layout::{PROJECT_STRUCT, StateFile};

    fn write(path: &Path, bytes: &[u8]) {
        std::fs::create_dir_all(path.parent().expect("parent")).expect("directory");
        std::fs::write(path, bytes).expect("writes");
    }

    fn version_two(root: &Path) {
        let mut file = StateFile::current();
        file.schemas.insert(PROJECT_STRUCT.to_owned(), 2);
        file.write(root).expect("writes layout");
    }

    #[test]
    fn the_version_two_upgrade_keeps_profiles_and_dialogs_and_repoints_skill_rows() {
        let home = tempfile::tempdir().expect("temporary local data");
        let layout = StudioLayout::resolve(home.path(), false);
        let old = layout.previous_desktop.as_deref().expect("old root");
        version_two(old);
        let browser = Path::new("profiles/browser/work/profile");
        let dialog = Path::new("data/conversations/owned/history.jsonl");
        write(&old.join(browser), b"browser profile");
        write(&old.join(dialog), b"saved history\n");
        write(&old.join("data/library/skills/example/SKILL.md"), b"skill");
        std::fs::create_dir(&layout.desktop).expect("empty destination");
        let database = old.join("data/studio.sqlite");
        let connection = rusqlite::Connection::open(&database).expect("opens database");
        connection.execute_batch(
            "CREATE TABLE skill (path TEXT); INSERT INTO skill VALUES ('data:library/skills/example');",
        ).expect("legacy skill");
        drop(connection);

        let boot = Boot::new(&layout.desktop, None).with_layout(&layout);
        let decision = boot.inspect().expect("inspects");
        assert!(
            matches!(&decision, BootDecision::Migrate(steps) if steps.len() == 1 && steps[0].from == 2)
        );
        boot.run(&decision, 0, &|_| {}).expect("upgrades");
        assert_eq!(boot.inspect().expect("inspects"), BootDecision::Ready);
        assert!(!old.exists());
        assert_eq!(
            std::fs::read(layout.desktop.join(browser)).expect("profile"),
            b"browser profile"
        );
        assert_eq!(
            std::fs::read(layout.desktop.join(dialog)).expect("history"),
            b"saved history\n"
        );
        assert!(layout.shared.skills.join("example/SKILL.md").is_file());
        let connection = rusqlite::Connection::open(layout.desktop.join("data/studio.sqlite"))
            .expect("database");
        let path: String = connection
            .query_row("SELECT path FROM skill", [], |row| row.get(0))
            .expect("skill");
        assert_eq!(path, "skills:example");
    }

    #[test]
    fn a_conflicting_shared_skill_preserves_both_and_the_upgrade_can_resume() {
        let home = tempfile::tempdir().expect("temporary local data");
        let layout = StudioLayout::resolve(home.path(), false);
        let old = layout.previous_desktop.as_deref().expect("old root");
        version_two(old);
        let desktop_skill = Path::new("data/library/skills/example/SKILL.md");
        write(&old.join(desktop_skill), b"old");
        let existing = layout.shared.skills.join("example/SKILL.md");
        write(&existing, b"existing");
        let boot = Boot::new(&layout.desktop, None).with_layout(&layout);
        let decision = boot.inspect().expect("inspects");
        assert!(matches!(
            boot.run(&decision, 0, &|_| {}),
            Err(BootError::DestinationConflict(_))
        ));
        assert_eq!(std::fs::read(&existing).expect("shared copy"), b"existing");
        assert_eq!(
            std::fs::read(layout.desktop.join(desktop_skill)).expect("desktop copy"),
            b"old"
        );
        assert_eq!(boot.inspect().expect("inspects"), decision);
        std::fs::rename(
            existing.parent().expect("skill"),
            layout.shared.skills.join("renamed"),
        )
        .expect("resolves conflict");
        boot.run(&decision, 0, &|_| {}).expect("resumes");
        assert_eq!(std::fs::read(&existing).expect("moved skill"), b"old");
        assert_eq!(boot.inspect().expect("inspects"), BootDecision::Ready);
    }
}
