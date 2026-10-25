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
            allowed_tools: None,
            model_override: None,
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
