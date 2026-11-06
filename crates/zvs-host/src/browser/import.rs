use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use zvs_contract::{
    AppError, AppErrorCode, BrowserImportInput, BrowserImportKind, BrowserImportResultDto,
    BrowserImportSourceDto, BrowserImportSourcesDto, Validate,
};

use super::library::{BrowserLibrary, ImportedBookmark, ImportedVisit};

pub const IMPORT_SOURCE_GONE: &str = "Браузер для импорта не найден";
pub const IMPORT_READ_FAILED: &str =
    "Не удалось прочитать данные браузера. Закройте его и повторите попытку.";
pub const IMPORT_VISITS_MAX: usize = 10_000;

const WEBKIT_EPOCH_OFFSET_MS: i64 = 11_644_473_600_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Root {
    Local,
    Roaming,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Layout {
    Chromium,
    SingleChromium,
    Firefox,
}

const FAMILIES: [(BrowserImportKind, Root, &str, Layout); 7] = [
    (
        BrowserImportKind::Chrome,
        Root::Local,
        "Google/Chrome/User Data",
        Layout::Chromium,
    ),
    (
        BrowserImportKind::Edge,
        Root::Local,
        "Microsoft/Edge/User Data",
        Layout::Chromium,
    ),
    (
        BrowserImportKind::Brave,
        Root::Local,
        "BraveSoftware/Brave-Browser/User Data",
        Layout::Chromium,
    ),
    (
        BrowserImportKind::Vivaldi,
        Root::Local,
        "Vivaldi/User Data",
        Layout::Chromium,
    ),
    (
        BrowserImportKind::Yandex,
        Root::Local,
        "Yandex/YandexBrowser/User Data",
        Layout::Chromium,
    ),
    (
        BrowserImportKind::Opera,
        Root::Roaming,
        "Opera Software/Opera Stable",
        Layout::SingleChromium,
    ),
    (
        BrowserImportKind::Firefox,
        Root::Roaming,
        "Mozilla/Firefox/Profiles",
        Layout::Firefox,
    ),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportRoots {
    pub local: Option<PathBuf>,
    pub roaming: Option<PathBuf>,
}

impl ImportRoots {
    pub fn system() -> Self {
        Self {
            local: std::env::var_os("LOCALAPPDATA").map(PathBuf::from),
            roaming: std::env::var_os("APPDATA").map(PathBuf::from),
        }
    }

    fn of(&self, root: Root) -> Option<&Path> {
        match root {
            Root::Local => self.local.as_deref(),
            Root::Roaming => self.roaming.as_deref(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Source {
    dto: BrowserImportSourceDto,
    dir: PathBuf,
    layout: Layout,
}

fn joined(base: &Path, relative: &str) -> PathBuf {
    relative
        .split('/')
        .fold(base.to_path_buf(), |path, part| path.join(part))
}

fn chromium_names(user_data: &Path) -> HashMap<String, String> {
    std::fs::read_to_string(user_data.join("Local State"))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|state| state.pointer("/profile/info_cache").cloned())
        .and_then(|cache| cache.as_object().cloned())
        .map(|cache| {
            cache
                .into_iter()
                .filter_map(|(dir, info)| {
                    info.get("name")
                        .and_then(Value::as_str)
                        .map(|name| (dir, name.to_owned()))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn chromium_source(
    kind: BrowserImportKind,
    key: &str,
    label: String,
    dir: PathBuf,
    layout: Layout,
) -> Option<Source> {
    let history = dir.join("History").is_file();
    let bookmarks = dir.join("Bookmarks").is_file();
    (history || bookmarks).then(|| Source {
        dto: BrowserImportSourceDto {
            id: format!("{}:{key}", kind.as_str()),
            kind,
            profile: label,
            history,
            bookmarks,
        },
        dir,
        layout,
    })
}

fn sorted_dirs(path: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(path)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.is_dir())
                .collect()
        })
        .unwrap_or_default();
    dirs.sort();
    dirs
}

fn dir_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn discover(roots: &ImportRoots) -> Vec<Source> {
    let mut sources = Vec::new();
    for (kind, root, relative, layout) in FAMILIES {
        let Some(base) = roots.of(root) else {
            continue;
        };
        let home = joined(base, relative);
        match layout {
            Layout::Chromium => {
                let names = chromium_names(&home);
                for dir in sorted_dirs(&home) {
                    let key = dir_name(&dir);
                    if key == "System Profile" || key == "Guest Profile" {
                        continue;
                    }
                    let label = names.get(&key).cloned().unwrap_or_else(|| key.clone());
                    sources.extend(chromium_source(kind, &key, label, dir, layout));
                }
            }
            Layout::SingleChromium => {
                sources.extend(chromium_source(kind, "", String::new(), home, layout));
            }
            Layout::Firefox => {
                for dir in sorted_dirs(&home) {
                    if !dir.join("places.sqlite").is_file() {
                        continue;
                    }
                    let key = dir_name(&dir);
                    let label = key
                        .split_once('.')
                        .map_or_else(|| key.clone(), |(_, name)| name.to_owned());
                    sources.push(Source {
                        dto: BrowserImportSourceDto {
                            id: format!("{}:{key}", kind.as_str()),
                            kind,
                            profile: label,
                            history: true,
                            bookmarks: true,
                        },
                        dir,
                        layout,
                    });
                }
            }
        }
    }
    sources
}

fn read_failed(cause: impl ToString) -> AppError {
    AppError::new(AppErrorCode::Conflict, IMPORT_READ_FAILED)
        .with_detail("reason", cause.to_string())
}

struct Snapshot {
    _folder: tempfile::TempDir,
    connection: Connection,
}

fn snapshot(database: &Path) -> Result<Snapshot, AppError> {
    let folder = tempfile::tempdir().map_err(read_failed)?;
    let copy = folder.path().join("copy.sqlite");
    std::fs::copy(database, &copy).map_err(read_failed)?;
    for suffix in ["-wal", "-journal"] {
        let mut companion = database.as_os_str().to_owned();
        companion.push(suffix);
        let companion = PathBuf::from(companion);
        if companion.is_file() {
            let mut target = copy.as_os_str().to_owned();
            target.push(suffix);
            std::fs::copy(&companion, PathBuf::from(target)).map_err(read_failed)?;
        }
    }
    let connection = Connection::open_with_flags(&copy, OpenFlags::SQLITE_OPEN_READ_WRITE)
        .map_err(read_failed)?;
    Ok(Snapshot {
        _folder: folder,
        connection,
    })
}

fn limit() -> i64 {
    i64::try_from(IMPORT_VISITS_MAX).unwrap_or(i64::MAX)
}

fn chromium_history(dir: &Path) -> Result<Vec<ImportedVisit>, AppError> {
    let snapshot = snapshot(&dir.join("History"))?;
    let mut statement = snapshot
        .connection
        .prepare(
            "SELECT url, COALESCE(title, ''), last_visit_time FROM urls \
             WHERE last_visit_time > 0 ORDER BY last_visit_time DESC LIMIT ?1",
        )
        .map_err(read_failed)?;
    let rows = statement
        .query_map([limit()], |row| {
            Ok(ImportedVisit {
                url: row.get(0)?,
                title: row.get(1)?,
                visited_at: row.get::<_, i64>(2)? / 1000 - WEBKIT_EPOCH_OFFSET_MS,
            })
        })
        .map_err(read_failed)?;
    rows.collect::<Result<_, _>>().map_err(read_failed)
}

const CHROMIUM_ROOTS: [(&str, &str); 3] = [
    ("bookmark_bar", "Панель закладок"),
    ("other", "Другие закладки"),
    ("synced", "Мобильные закладки"),
];

fn walk_chromium(node: &Value, folder: &str, out: &mut Vec<ImportedBookmark>) {
    let name = node.get("name").and_then(Value::as_str).unwrap_or_default();
    match node.get("type").and_then(Value::as_str) {
        Some("url") => {
            if let Some(url) = node.get("url").and_then(Value::as_str) {
                out.push(ImportedBookmark {
                    url: url.to_owned(),
                    title: name.to_owned(),
                    folder: folder.to_owned(),
                });
            }
        }
        Some("folder") => {
            let inner = if folder.is_empty() {
                name.to_owned()
            } else {
                format!("{folder}/{name}")
            };
            for child in node
                .get("children")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                walk_chromium(child, &inner, out);
            }
        }
        _ => {}
    }
}

fn chromium_bookmarks(dir: &Path) -> Result<Vec<ImportedBookmark>, AppError> {
    let text = std::fs::read_to_string(dir.join("Bookmarks")).map_err(read_failed)?;
    let document: Value = serde_json::from_str(&text).map_err(read_failed)?;
    let mut out = Vec::new();
    for (key, label) in CHROMIUM_ROOTS {
        if let Some(root) = document.pointer(&format!("/roots/{key}")) {
            for child in root
                .get("children")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                walk_chromium(child, label, &mut out);
            }
        }
    }
    Ok(out)
}

fn firefox_history(dir: &Path) -> Result<Vec<ImportedVisit>, AppError> {
    let snapshot = snapshot(&dir.join("places.sqlite"))?;
    let mut statement = snapshot
        .connection
        .prepare(
            "SELECT url, COALESCE(title, ''), last_visit_date FROM moz_places \
             WHERE last_visit_date IS NOT NULL ORDER BY last_visit_date DESC LIMIT ?1",
        )
        .map_err(read_failed)?;
    let rows = statement
        .query_map([limit()], |row| {
            Ok(ImportedVisit {
                url: row.get(0)?,
                title: row.get(1)?,
                visited_at: row.get::<_, i64>(2)? / 1000,
            })
        })
        .map_err(read_failed)?;
    rows.collect::<Result<_, _>>().map_err(read_failed)
}

fn firefox_root(title: &str) -> Option<&'static str> {
    match title {
        "toolbar" => Some("Панель закладок"),
        "menu" => Some("Меню закладок"),
        "unfiled" => Some("Другие закладки"),
        "mobile" => Some("Мобильные закладки"),
        _ => None,
    }
}

fn firefox_bookmarks(dir: &Path) -> Result<Vec<ImportedBookmark>, AppError> {
    let snapshot = snapshot(&dir.join("places.sqlite"))?;
    let mut statement = snapshot
        .connection
        .prepare(
            "SELECT b.id, COALESCE(b.parent, 0), b.type, COALESCE(b.title, ''), p.url \
             FROM moz_bookmarks b LEFT JOIN moz_places p ON p.id = b.fk",
        )
        .map_err(read_failed)?;
    let rows: Vec<(i64, i64, i64, String, Option<String>)> = statement
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })
        .map_err(read_failed)?
        .collect::<Result<_, _>>()
        .map_err(read_failed)?;
    let folders: HashMap<i64, (i64, String)> = rows
        .iter()
        .filter(|row| row.2 == 2)
        .map(|row| (row.0, (row.1, row.3.clone())))
        .collect();
    let path_of = |mut parent: i64| {
        let mut parts = Vec::new();
        let mut guard = 0;
        while let Some((up, title)) = folders.get(&parent) {
            if *up == 0 || guard > 64 {
                break;
            }
            let up_is_root = folders.get(up).is_none_or(|(above, _)| *above == 0);
            parts.push(if up_is_root {
                firefox_root(title).unwrap_or(title).to_owned()
            } else {
                title.clone()
            });
            parent = *up;
            guard += 1;
        }
        parts.reverse();
        parts.join("/")
    };
    Ok(rows
        .iter()
        .filter(|row| row.2 == 1)
        .filter_map(|row| {
            row.4.as_ref().map(|url| ImportedBookmark {
                url: url.clone(),
                title: row.3.clone(),
                folder: path_of(row.1),
            })
        })
        .collect())
}

#[derive(Clone)]
pub struct BrowserImporter {
    roots: ImportRoots,
    library: BrowserLibrary,
}

impl BrowserImporter {
    pub const fn new(roots: ImportRoots, library: BrowserLibrary) -> Self {
        Self { roots, library }
    }

    pub fn sources(&self) -> BrowserImportSourcesDto {
        BrowserImportSourcesDto {
            sources: discover(&self.roots)
                .into_iter()
                .map(|source| source.dto)
                .collect(),
        }
    }

    pub fn run(&self, input: &BrowserImportInput) -> Result<BrowserImportResultDto, AppError> {
        input.validate()?;
        let source = discover(&self.roots)
            .into_iter()
            .find(|source| source.dto.id == input.source)
            .ok_or_else(|| AppError::new(AppErrorCode::NotFound, IMPORT_SOURCE_GONE))?;
        let mut result = BrowserImportResultDto::default();
        if input.history && source.dto.history {
            let visits = match source.layout {
                Layout::Firefox => firefox_history(&source.dir)?,
                Layout::Chromium | Layout::SingleChromium => chromium_history(&source.dir)?,
            };
            result.history = self.library.import_history(source.dto.kind, &visits)?;
        }
        if input.bookmarks && source.dto.bookmarks {
            let bookmarks = match source.layout {
                Layout::Firefox => firefox_bookmarks(&source.dir)?,
                Layout::Chromium | Layout::SingleChromium => chromium_bookmarks(&source.dir)?,
            };
            result.bookmarks = self.library.import_bookmarks(&bookmarks)?;
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;

    use super::*;
    use crate::platform::clock::FakeClock;
    use crate::testing::temporary_database;

    fn write_chrome_profile(user_data: &Path) {
        let profile = user_data.join("Default");
        std::fs::create_dir_all(&profile).expect("creates");
        std::fs::write(
            user_data.join("Local State"),
            json!({ "profile": { "info_cache": { "Default": { "name": "Работа" } } } }).to_string(),
        )
        .expect("writes");
        let history = Connection::open(profile.join("History")).expect("opens");
        history
            .execute_batch(&format!(
                "CREATE TABLE urls (id INTEGER PRIMARY KEY, url TEXT, title TEXT, last_visit_time INTEGER);
                 INSERT INTO urls (url, title, last_visit_time) VALUES ('https://a.example/', 'A', {}), ('chrome://newtab/', 'New', 1);",
                (1_700_000_000_000 + WEBKIT_EPOCH_OFFSET_MS) * 1000
            ))
            .expect("seeds");
        std::fs::write(
            profile.join("Bookmarks"),
            json!({ "roots": {
                "bookmark_bar": { "type": "folder", "name": "Bookmarks bar", "children": [
                    { "type": "url", "name": "Docs", "url": "https://docs.example/" },
                    { "type": "folder", "name": "Работа", "children": [
                        { "type": "url", "name": "Jira", "url": "https://jira.example/" }
                    ] }
                ] },
                "other": { "type": "folder", "name": "Other", "children": [] }
            } })
            .to_string(),
        )
        .expect("writes");
        std::fs::create_dir_all(user_data.join("System Profile")).expect("creates");
    }

    fn write_firefox_profile(profiles: &Path) {
        let profile = profiles.join("abcd1234.default-release");
        std::fs::create_dir_all(&profile).expect("creates");
        let places = Connection::open(profile.join("places.sqlite")).expect("opens");
        places
            .execute_batch(
                "CREATE TABLE moz_places (id INTEGER PRIMARY KEY, url TEXT, title TEXT, last_visit_date INTEGER);
                 CREATE TABLE moz_bookmarks (id INTEGER PRIMARY KEY, type INTEGER, fk INTEGER, parent INTEGER, title TEXT);
                 INSERT INTO moz_places VALUES (1, 'https://mdn.example/', 'MDN', 1700000000000000), (2, 'https://news.example/', NULL, NULL);
                 INSERT INTO moz_bookmarks VALUES (1, 2, NULL, 0, ''), (3, 2, NULL, 1, 'toolbar'), (10, 2, NULL, 3, 'Новости'), (11, 1, 2, 10, 'News'), (12, 1, 1, 3, 'MDN');",
            )
            .expect("seeds");
    }

    fn importer() -> (
        tempfile::TempDir,
        crate::testing::TemporaryDatabase,
        BrowserImporter,
    ) {
        let home = tempfile::tempdir().expect("a home");
        let local = home.path().join("Local");
        let roaming = home.path().join("Roaming");
        write_chrome_profile(&joined(&local, "Google/Chrome/User Data"));
        write_firefox_profile(&joined(&roaming, "Mozilla/Firefox/Profiles"));
        let database = temporary_database();
        let library = BrowserLibrary::new(database.shared(), Arc::new(FakeClock::default()));
        let importer = BrowserImporter::new(
            ImportRoots {
                local: Some(local),
                roaming: Some(roaming),
            },
            library,
        );
        (home, database, importer)
    }

    #[test]
    fn installed_profiles_are_found_with_their_names() {
        let (_home, _database, importer) = importer();
        let sources = importer.sources().sources;
        assert_eq!(
            sources
                .iter()
                .map(|source| (source.id.as_str(), source.profile.as_str()))
                .collect::<Vec<_>>(),
            [
                ("chrome:Default", "Работа"),
                ("firefox:abcd1234.default-release", "default-release"),
            ]
        );
    }

    #[test]
    fn chrome_history_and_bookmarks_are_imported_once() {
        let (_home, _database, importer) = importer();
        let input = BrowserImportInput {
            source: "chrome:Default".to_owned(),
            history: true,
            bookmarks: true,
        };
        assert_eq!(
            importer.run(&input).expect("imports"),
            BrowserImportResultDto {
                history: 1,
                bookmarks: 2
            }
        );
        assert_eq!(
            importer.run(&input).expect("imports again"),
            BrowserImportResultDto::default()
        );
        let history = importer.library.history(None).expect("lists");
        assert_eq!(history.items[0].visited_at, 1_700_000_000_000);
        let folders: Vec<String> = importer
            .library
            .bookmarks()
            .expect("lists")
            .items
            .into_iter()
            .map(|item| item.folder)
            .collect();
        assert_eq!(folders, ["Панель закладок", "Панель закладок/Работа"]);
    }

    #[test]
    fn firefox_places_are_read_with_their_folders() {
        let (_home, _database, importer) = importer();
        let result = importer
            .run(&BrowserImportInput {
                source: "firefox:abcd1234.default-release".to_owned(),
                history: true,
                bookmarks: true,
            })
            .expect("imports");
        assert_eq!(
            result,
            BrowserImportResultDto {
                history: 1,
                bookmarks: 2
            }
        );
        let bookmarks = importer.library.bookmarks().expect("lists").items;
        assert!(
            bookmarks
                .iter()
                .any(|item| item.url == "https://news.example/"
                    && item.folder == "Панель закладок/Новости")
        );
        assert_eq!(
            importer
                .run(&BrowserImportInput {
                    source: "edge:Default".to_owned(),
                    history: true,
                    bookmarks: false,
                })
                .expect_err("not installed")
                .message,
            IMPORT_SOURCE_GONE
        );
    }
}
