use std::sync::Arc;

use zvs_contract::browser_library::{BOOKMARKS_MAX, HISTORY_PAGE_MAX, HISTORY_TITLE_MAX};
use zvs_contract::validate::{js_slice, js_trim};
use zvs_contract::{
    AppError, AppErrorCode, BrowserBookmarkDto, BrowserBookmarkLink, BrowserBookmarksDto,
    BrowserHistoryDto, BrowserHistoryEntryDto, BrowserImportKind, Timestamp,
};

use super::policy::is_web_url;
use crate::data::schema::{BrowserBookmarkRow, BrowserHistoryRow};
use crate::data::{Database, Repositories};
use crate::platform::clock::SharedClock;
use crate::platform::ids::new_id;

pub const HISTORY_KEPT: usize = 20_000;
pub const VISIT_MERGE_MS: Timestamp = 30 * 60 * 1000;
pub const CHROME_BOOKMARKS: usize = 24;
pub const BOOKMARK_NOT_FOUND: &str = "Закладка не найдена";
pub const BOOKMARKS_FULL: &str = "Достигнуто максимальное число закладок";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedVisit {
    pub url: String,
    pub title: String,
    pub visited_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedBookmark {
    pub url: String,
    pub title: String,
    pub folder: String,
}

fn title_of(title: &str) -> String {
    js_slice(js_trim(title), HISTORY_TITLE_MAX).to_owned()
}

fn history_dto(row: BrowserHistoryRow) -> BrowserHistoryEntryDto {
    BrowserHistoryEntryDto {
        id: row.id,
        url: row.url,
        title: row.title,
        visited_at: row.visited_at,
        imported_from: row
            .imported_from
            .as_deref()
            .and_then(BrowserImportKind::parse),
    }
}

fn bookmark_dto(row: BrowserBookmarkRow) -> BrowserBookmarkDto {
    BrowserBookmarkDto {
        id: row.id,
        url: row.url,
        title: row.title,
        folder: row.folder,
        created_at: row.created_at,
    }
}

#[derive(Clone)]
pub struct BrowserLibrary {
    database: Arc<Database>,
    clock: SharedClock,
}

impl BrowserLibrary {
    pub fn new(database: Arc<Database>, clock: SharedClock) -> Self {
        Self { database, clock }
    }

    pub fn visit(&self, url: &str, title: &str) -> Result<(), AppError> {
        if !is_web_url(url) {
            return Ok(());
        }
        let now = self.clock.now();
        let title = title_of(title);
        self.database.with_connection(|connection| {
            let history = Repositories::new(connection).browser_history;
            match history.latest_for(url)? {
                Some(last) if now - last.visited_at < VISIT_MERGE_MS => {
                    let title = if title.is_empty() { last.title } else { title };
                    history.touch(&last.id, &title, now)?;
                }
                _ => {
                    history.insert(&BrowserHistoryRow {
                        id: new_id().hyphenated().to_string(),
                        url: url.to_owned(),
                        title,
                        visited_at: now,
                        imported_from: None,
                    })?;
                    history.keep_newest(HISTORY_KEPT)?;
                }
            }
            Ok(())
        })
    }

    pub fn history(&self, query: Option<&str>) -> Result<BrowserHistoryDto, AppError> {
        let needle = query.map(js_trim).unwrap_or_default().to_lowercase();
        self.database.with_connection(|connection| {
            let history = Repositories::new(connection).browser_history;
            if needle.is_empty() {
                return Ok(BrowserHistoryDto {
                    items: history
                        .newest(HISTORY_PAGE_MAX)?
                        .into_iter()
                        .map(history_dto)
                        .collect(),
                    total: history.total()?,
                });
            }
            let matching: Vec<BrowserHistoryRow> = history
                .newest(HISTORY_KEPT)?
                .into_iter()
                .filter(|row| {
                    row.url.to_lowercase().contains(&needle)
                        || row.title.to_lowercase().contains(&needle)
                })
                .collect();
            Ok(BrowserHistoryDto {
                total: u64::try_from(matching.len()).unwrap_or(u64::MAX),
                items: matching
                    .into_iter()
                    .take(HISTORY_PAGE_MAX)
                    .map(history_dto)
                    .collect(),
            })
        })
    }

    pub fn remove_visit(&self, id: &str) -> Result<(), AppError> {
        self.database.with_connection(|connection| {
            Repositories::new(connection).browser_history.remove(id)?;
            Ok(())
        })
    }

    pub fn clear_history(&self) -> Result<(), AppError> {
        self.database.with_connection(|connection| {
            Repositories::new(connection).browser_history.clear()?;
            Ok(())
        })
    }

    pub fn import_history(
        &self,
        kind: BrowserImportKind,
        visits: &[ImportedVisit],
    ) -> Result<u64, AppError> {
        self.database.transaction(|transaction| {
            let history = Repositories::new(transaction).browser_history;
            let mut added = 0;
            for visit in visits.iter().filter(|visit| is_web_url(&visit.url)) {
                if history.insert(&BrowserHistoryRow {
                    id: new_id().hyphenated().to_string(),
                    url: visit.url.clone(),
                    title: title_of(&visit.title),
                    visited_at: visit.visited_at.max(0),
                    imported_from: Some(kind.as_str().to_owned()),
                })? {
                    added += 1;
                }
            }
            history.keep_newest(HISTORY_KEPT)?;
            Ok(added)
        })
    }

    pub fn bookmarks(&self) -> Result<BrowserBookmarksDto, AppError> {
        self.database.with_connection(|connection| {
            Ok(BrowserBookmarksDto {
                items: Repositories::new(connection)
                    .browser_bookmarks
                    .list(BOOKMARKS_MAX)?
                    .into_iter()
                    .map(bookmark_dto)
                    .collect(),
            })
        })
    }

    pub fn links(&self) -> Vec<BrowserBookmarkLink> {
        self.database
            .with_connection(|connection| {
                Ok(Repositories::new(connection)
                    .browser_bookmarks
                    .recent(CHROME_BOOKMARKS)?)
            })
            .unwrap_or_default()
            .into_iter()
            .map(|row| BrowserBookmarkLink {
                id: row.id,
                title: row.title,
                url: row.url,
            })
            .collect()
    }

    pub fn is_bookmarked(&self, url: &str) -> bool {
        self.database
            .with_connection(|connection| {
                Ok(Repositories::new(connection)
                    .browser_bookmarks
                    .find_url(url)?)
            })
            .ok()
            .flatten()
            .is_some()
    }

    pub fn toggle(&self, url: &str, title: &str) -> Result<bool, AppError> {
        if !is_web_url(url) {
            return Ok(false);
        }
        let now = self.clock.now();
        self.database.with_connection(|connection| {
            let bookmarks = Repositories::new(connection).browser_bookmarks;
            if let Some(existing) = bookmarks.find_url(url)? {
                bookmarks.remove(&existing.id)?;
                return Ok(false);
            }
            if bookmarks.count()? >= BOOKMARKS_MAX {
                return Err(AppError::new(AppErrorCode::Conflict, BOOKMARKS_FULL).into());
            }
            let title = title_of(title);
            bookmarks.insert(&BrowserBookmarkRow {
                id: new_id().hyphenated().to_string(),
                url: url.to_owned(),
                title: if title.is_empty() {
                    url.to_owned()
                } else {
                    title
                },
                folder: String::new(),
                created_at: now,
            })?;
            Ok(true)
        })
    }

    pub fn rename(&self, id: &str, title: &str) -> Result<BrowserBookmarksDto, AppError> {
        let title = title_of(title);
        let renamed = self.database.with_connection(|connection| {
            Ok(Repositories::new(connection)
                .browser_bookmarks
                .rename(id, &title)?)
        })?;
        if !renamed || title.is_empty() {
            return Err(AppError::new(AppErrorCode::NotFound, BOOKMARK_NOT_FOUND));
        }
        self.bookmarks()
    }

    pub fn remove_bookmark(&self, id: &str) -> Result<BrowserBookmarksDto, AppError> {
        self.database.with_connection(|connection| {
            Repositories::new(connection).browser_bookmarks.remove(id)?;
            Ok(())
        })?;
        self.bookmarks()
    }

    pub fn import_bookmarks(&self, items: &[ImportedBookmark]) -> Result<u64, AppError> {
        let now = self.clock.now();
        self.database.transaction(|transaction| {
            let bookmarks = Repositories::new(transaction).browser_bookmarks;
            let mut room = BOOKMARKS_MAX.saturating_sub(bookmarks.count()?);
            let mut added = 0;
            for item in items.iter().filter(|item| is_web_url(&item.url)) {
                if room == 0 {
                    break;
                }
                let title = title_of(&item.title);
                if bookmarks.insert(&BrowserBookmarkRow {
                    id: new_id().hyphenated().to_string(),
                    url: item.url.clone(),
                    title: if title.is_empty() {
                        item.url.clone()
                    } else {
                        title
                    },
                    folder: js_slice(&item.folder, 1024).to_owned(),
                    created_at: now,
                })? {
                    added += 1;
                    room -= 1;
                }
            }
            Ok(added)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::clock::FakeClock;
    use crate::testing::temporary_database;

    fn library() -> (
        crate::testing::TemporaryDatabase,
        Arc<FakeClock>,
        BrowserLibrary,
    ) {
        let database = temporary_database();
        let clock = Arc::new(FakeClock::default());
        let library = BrowserLibrary::new(database.shared(), clock.clone());
        (database, clock, library)
    }

    #[test]
    fn repeated_visits_merge_and_a_later_one_starts_a_new_entry() {
        let (_database, clock, library) = library();
        library
            .visit("https://docs.example/a", "")
            .expect("records");
        library
            .visit("https://docs.example/a", "Документация")
            .expect("records");
        library.visit("about:blank", "Пусто").expect("ignored");
        let history = library.history(None).expect("lists");
        assert_eq!(history.total, 1);
        assert_eq!(history.items[0].title, "Документация");
        clock.advance(VISIT_MERGE_MS + 1);
        library
            .visit("https://docs.example/a", "")
            .expect("records");
        let history = library.history(None).expect("lists");
        assert_eq!(history.total, 2);
        assert_eq!(history.items[0].title, "");
        assert_eq!(library.history(Some("ДОКУМЕН")).expect("lists").total, 1);
        library.remove_visit(&history.items[0].id).expect("removes");
        assert_eq!(library.history(None).expect("lists").total, 1);
        library.clear_history().expect("clears");
        assert_eq!(library.history(None).expect("lists").total, 0);
    }

    #[test]
    fn imported_visits_keep_their_time_and_source_and_skip_duplicates() {
        let (_database, _clock, library) = library();
        let visits = [
            ImportedVisit {
                url: "https://a.example/".to_owned(),
                title: " A ".to_owned(),
                visited_at: 1_000,
            },
            ImportedVisit {
                url: "chrome://settings".to_owned(),
                title: String::new(),
                visited_at: 2_000,
            },
        ];
        assert_eq!(
            library
                .import_history(BrowserImportKind::Chrome, &visits)
                .expect("imports"),
            1
        );
        assert_eq!(
            library
                .import_history(BrowserImportKind::Chrome, &visits)
                .expect("imports"),
            0
        );
        let entry = &library.history(None).expect("lists").items[0];
        assert_eq!(entry.title, "A");
        assert_eq!(entry.visited_at, 1_000);
        assert_eq!(entry.imported_from, Some(BrowserImportKind::Chrome));
    }

    #[test]
    fn the_star_toggles_and_imports_keep_folders() {
        let (_database, _clock, library) = library();
        assert!(library.toggle("https://a.example/", "A").expect("adds"));
        assert!(library.is_bookmarked("https://a.example/"));
        assert_eq!(library.links().len(), 1);
        assert!(!library.toggle("https://a.example/", "A").expect("removes"));
        assert!(!library.toggle("file:///C:/", "x").expect("ignored"));
        let added = library
            .import_bookmarks(&[
                ImportedBookmark {
                    url: "https://b.example/".to_owned(),
                    title: String::new(),
                    folder: "Панель закладок/Работа".to_owned(),
                },
                ImportedBookmark {
                    url: "https://b.example/".to_owned(),
                    title: "B".to_owned(),
                    folder: String::new(),
                },
            ])
            .expect("imports");
        assert_eq!(added, 1);
        let items = library.bookmarks().expect("lists").items;
        assert_eq!(items[0].title, "https://b.example/");
        assert_eq!(items[0].folder, "Панель закладок/Работа");
        let renamed = library.rename(&items[0].id, "Работа").expect("renames");
        assert_eq!(renamed.items[0].title, "Работа");
        assert!(
            library
                .remove_bookmark(&items[0].id)
                .expect("removes")
                .items
                .is_empty()
        );
        assert_eq!(
            library.rename(&items[0].id, "x").expect_err("gone").message,
            BOOKMARK_NOT_FOUND
        );
    }
}
