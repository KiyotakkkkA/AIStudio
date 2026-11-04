use std::sync::Arc;
use std::time::Duration;

use zvs_contract::{CreateSkillInput, SkillFileInput};

use super::*;
use crate::platform::clock::FakeClock;
use crate::platform::events::EventBus;
use crate::services::SystemService;
use crate::skills::library::SKILL_NOT_FOUND;
use crate::testing::{TestServices, temporary_directory, test_services};

const CHAT: &str = "01a17431-1de8-713c-9981-dbd5ddd5b80b";
const KEY: &str = "extract";

#[test]
fn absolute_paths_resolve_across_the_entire_owned_conversation_directory() {
    let root = temporary_directory("owned-roots");
    let store = ArtifactStore::new(root.path().join("conversations"));
    let roots = VirtualRoots::new(Some(store.clone()), None);
    let owner = store.folders().folder(CHAT).unwrap();
    let image = owner.join("attachments/browser/screen-test.jpg");
    std::fs::create_dir_all(image.parent().unwrap()).unwrap();
    std::fs::write(&image, [255, 216, 255, 217]).unwrap();
    let plain = crate::workspace::canonical(&image)
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let mut spellings = vec![
        plain.clone(),
        crate::platform::long_path::extended(Path::new(&plain))
            .to_string_lossy()
            .into_owned(),
    ];
    if cfg!(windows) {
        spellings.extend([plain.to_uppercase(), plain.replace('\\', "/")]);
    }
    for spelling in spellings {
        let located = roots.locate(CHAT, &spelling).unwrap().unwrap();
        assert_eq!(located.kind, RootKind::Chat);
        assert!(same_folder(
            &located.base,
            &crate::workspace::canonical(&owner).unwrap()
        ));
        assert!(same_folder(
            &located.path,
            &crate::workspace::canonical(&image).unwrap()
        ));
    }
    let missing = owner.join("outputs/new/note.txt");
    assert!(
        roots
            .locate(CHAT, missing.to_str().unwrap())
            .unwrap()
            .is_some()
    );
    assert!(!missing.exists());
    assert!(roots.locate("other", &plain).unwrap().is_none());
    let sibling = store
        .root()
        .join(format!("{CHAT}-other"))
        .join("secret.txt");
    assert!(
        roots
            .locate(CHAT, sibling.to_str().unwrap())
            .unwrap()
            .is_none()
    );
    let escaping = owner.join("..").join("other").join("secret.txt");
    assert!(
        roots
            .locate(CHAT, escaping.to_str().unwrap())
            .unwrap()
            .is_none()
    );
}

#[test]
fn owned_directory_links_cannot_escape_the_owner_boundary() {
    let root = temporary_directory("owned-links");
    let outside = temporary_directory("outside-owned");
    let store = ArtifactStore::new(root.path().join("conversations"));
    let roots = VirtualRoots::new(Some(store.clone()), None);
    let owner = store.folders().folder(CHAT).unwrap();
    std::fs::create_dir_all(&owner).unwrap();
    std::fs::write(outside.path().join("secret.txt"), "private").unwrap();
    for (link, scope) in [
        (owner.join("linked"), CHAT),
        (store.root().join("linked-owner"), "linked-owner"),
    ] {
        #[cfg(windows)]
        {
            let output = std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(&link)
                .arg(outside.path())
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path(), &link).unwrap();
        for name in ["secret.txt", "new/note.txt"] {
            assert_eq!(
                roots
                    .locate(scope, link.join(name).to_str().unwrap())
                    .unwrap_err()
                    .message,
                BAD_ROOT_PATH
            );
        }
    }
}

const ESCAPES: [&str; 9] = [
    "..",
    "../x.json",
    "a/../../x",
    "a\\..\\..\\x",
    ".hidden",
    "a/.git/config",
    "name.",
    "a|b",
    "C:/Windows/x",
];

fn host() -> TestServices {
    let host = test_services(SystemService::new(
        Arc::new(FakeClock::default()),
        EventBus::default(),
        Duration::ZERO,
    ));
    host.skills
        .create(&CreateSkillInput {
            key: KEY.to_owned(),
            name: "Извлечение".to_owned(),
            description: "Извлекает таблицы из документов".to_owned(),
            instructions: "Запустите scripts/extract.py.".to_owned(),
            enabled: None,
            files: Some(vec![SkillFileInput {
                path: "references/guide.md".to_owned(),
                content: "Руководство".to_owned(),
                encoding: None,
            }]),
        })
        .expect("creates the skill");
    host
}

#[test]
fn the_root_is_read_from_the_prefix() {
    assert_eq!(path_root("chat:_spec.json"), PathRoot::Chat("_spec.json"));
    assert_eq!(
        path_root(" chat:/data/_kpi.csv"),
        PathRoot::Chat("data/_kpi.csv")
    );
    assert_eq!(path_root("chat:"), PathRoot::Chat(""));
    assert_eq!(
        path_root("skill:dashboard/scripts/build.py"),
        PathRoot::Skill {
            key: "dashboard",
            rest: "scripts/build.py"
        }
    );
    assert_eq!(
        path_root("skill:dashboard\\references"),
        PathRoot::Skill {
            key: "dashboard",
            rest: "references"
        }
    );
    assert_eq!(
        path_root("skill:dashboard"),
        PathRoot::Skill {
            key: "dashboard",
            rest: ""
        }
    );
    for real in ["C:\\Users\\a\\file.txt", "src/main.rs", "/home/a", "c:file"] {
        assert_eq!(path_root(real), PathRoot::Real(real), "{real}");
        assert!(!is_virtual(real), "{real}");
    }
    assert_eq!(root_kind("chat:a"), Some(RootKind::Chat));
    assert_eq!(root_kind("skill:a/b"), Some(RootKind::Skill));
}

#[test]
fn every_escape_is_refused_under_a_base() {
    let base = Path::new("/data/artifacts/chats/c1");
    assert_eq!(under(base, "").expect("the base"), base);
    assert_eq!(
        under(base, "data\\_kpi.csv").expect("a path"),
        base.join("data").join("_kpi.csv")
    );
    for bad in ESCAPES {
        let error = under(base, bad).expect_err(bad);
        assert_eq!(error.message, BAD_ROOT_PATH, "{bad}");
    }
    assert!(under(base, &"a".repeat(ROOT_PATH_MAX + 1)).is_err());
}

#[test]
fn the_chat_root_resolves_inside_the_conversation_folder() {
    let root = temporary_directory("roots-chat-");
    let store = ArtifactStore::new(root.path().join("artifacts"));
    let roots = VirtualRoots::new(Some(store.clone()), None);
    let folder = chat_dir(&store, CHAT).expect("the chat folder");
    let located = roots
        .locate(CHAT, "chat:_work/_spec.json")
        .expect("resolves")
        .expect("a virtual path");
    assert_eq!(located.kind, RootKind::Chat);
    assert_eq!(located.base, folder);
    assert_eq!(located.path, folder.join("_work").join("_spec.json"));
    assert!(
        roots
            .locate(CHAT, "chat:")
            .expect("resolves")
            .expect("the folder")
            .is_base()
    );
    for bad in ESCAPES {
        let error = roots.locate(CHAT, &format!("chat:{bad}")).expect_err(bad);
        assert_eq!(error.message, BAD_ROOT_PATH, "{bad}");
    }
    assert!(roots.locate("", "chat:a.txt").is_err());
    assert!(roots.locate("../..", "chat:a.txt").is_err());
    assert_eq!(roots.locate(CHAT, "C:\\data\\a.txt").expect("real"), None);
    let missing = VirtualRoots::default()
        .locate(CHAT, "chat:a.txt")
        .expect_err("no store");
    assert_eq!(missing.message, ROOTS_UNAVAILABLE);
}

#[test]
fn the_skill_root_resolves_inside_a_usable_skill() {
    let host = host();
    let roots = VirtualRoots::new(None, Some(host.skills.library().clone()));
    let located = roots
        .locate(CHAT, "skill:extract/references/guide.md")
        .expect("resolves")
        .expect("a virtual path");
    assert_eq!(located.kind, RootKind::Skill);
    assert!(located.path.is_file());
    assert!(located.path.starts_with(&located.base));
    for bad in ESCAPES {
        let error = roots
            .locate(CHAT, &format!("skill:extract/{bad}"))
            .expect_err(bad);
        assert_eq!(error.message, BAD_ROOT_PATH, "{bad}");
    }
    let unknown = roots
        .locate(CHAT, "skill:absent/references/guide.md")
        .expect_err("an unknown skill");
    assert_eq!(unknown.message, SKILL_NOT_FOUND);
    let keyless = roots.locate(CHAT, "skill:").expect_err("no key");
    assert_eq!(keyless.message, SKILL_KEY_MISSING);
}
