use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};

use zvs_contract::Timestamp;

use crate::platform::ids::new_id;
use crate::platform::long_path::extended;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheEntry {
    pub hash: String,
    pub name: String,
    pub size: u64,
}

#[derive(Debug, Clone)]
pub struct DocumentCache {
    dir: PathBuf,
}

pub fn cache_file_name(hash: &str, extension: &str) -> String {
    format!("{hash}.{extension}")
}

fn parse_name(name: &str) -> Option<&str> {
    let (hash, extension) = name.split_once('.')?;
    let hashed = hash.len() == 64
        && hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    let extended = !extension.is_empty()
        && extension
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit());
    (hashed && extended).then_some(hash)
}

impl DocumentCache {
    pub const fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    pub fn contains(&self, name: &str) -> bool {
        extended(&self.path(name)).is_file()
    }

    pub fn temporary(&self, name: &str) -> PathBuf {
        self.path(&format!("{name}.{}.part", new_id().simple()))
    }

    pub fn store(&self, name: &str, bytes: &[u8]) -> io::Result<()> {
        std::fs::create_dir_all(extended(&self.dir))?;
        let temporary = self.temporary(name);
        std::fs::write(extended(&temporary), bytes)?;
        self.commit(&temporary, name)
    }

    pub fn commit(&self, temporary: &Path, name: &str) -> io::Result<()> {
        let target = self.path(name);
        match std::fs::rename(extended(temporary), extended(&target)) {
            Ok(()) => Ok(()),
            Err(_) if extended(&target).is_file() => {
                let _ = std::fs::remove_file(extended(temporary));
                Ok(())
            }
            Err(error) => {
                let _ = std::fs::remove_file(extended(temporary));
                Err(error)
            }
        }
    }

    pub fn entries(&self) -> io::Result<Vec<CacheEntry>> {
        let listing = match std::fs::read_dir(extended(&self.dir)) {
            Ok(listing) => listing,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        let mut entries = Vec::new();
        for entry in listing {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(hash) = parse_name(&name) else {
                continue;
            };
            let metadata = entry.metadata()?;
            if !metadata.is_file() {
                continue;
            }
            entries.push(CacheEntry {
                hash: hash.to_owned(),
                size: metadata.len(),
                name,
            });
        }
        entries.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(entries)
    }

    pub fn remove(&self, name: &str) -> io::Result<()> {
        match std::fs::remove_file(extended(&self.path(name))) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        }
    }
}

pub fn eviction_plan(
    entries: &[CacheEntry],
    last_opened: &HashMap<String, Timestamp>,
    protected: &HashSet<String>,
    ceiling: u64,
) -> Vec<CacheEntry> {
    let mut total: u64 = entries.iter().map(|entry| entry.size).sum();
    if total <= ceiling {
        return Vec::new();
    }
    let mut candidates: Vec<&CacheEntry> = entries
        .iter()
        .filter(|entry| !protected.contains(&entry.hash))
        .collect();
    candidates.sort_by_key(|entry| {
        (
            last_opened
                .get(&entry.hash)
                .copied()
                .unwrap_or(Timestamp::MIN),
            entry.name.clone(),
        )
    });
    let mut plan = Vec::new();
    for entry in candidates {
        if total <= ceiling {
            break;
        }
        total = total.saturating_sub(entry.size);
        plan.push(entry.clone());
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(seed: char) -> String {
        std::iter::repeat_n(seed, 64).collect()
    }

    fn entry(seed: char, size: u64) -> CacheEntry {
        CacheEntry {
            hash: hash(seed),
            name: cache_file_name(&hash(seed), "pdf"),
            size,
        }
    }

    #[test]
    fn evicts_the_least_recently_opened_until_under_the_ceiling() {
        let entries = [entry('a', 40), entry('b', 40), entry('c', 40)];
        let opened = HashMap::from([(hash('a'), 30), (hash('b'), 10), (hash('c'), 20)]);
        let plan = eviction_plan(&entries, &opened, &HashSet::new(), 80);
        assert_eq!(plan, [entry('b', 40)]);
        assert!(eviction_plan(&entries, &opened, &HashSet::new(), 120).is_empty());
    }

    #[test]
    fn an_open_document_is_never_planned_for_eviction() {
        let entries = [entry('a', 40), entry('b', 40)];
        let opened = HashMap::from([(hash('a'), 1), (hash('b'), 2)]);
        let protected = HashSet::from([hash('a'), hash('b')]);
        assert!(eviction_plan(&entries, &opened, &protected, 0).is_empty());
    }

    #[test]
    fn files_nobody_recorded_go_first() {
        let entries = [entry('a', 40), entry('b', 40)];
        let opened = HashMap::from([(hash('a'), 1)]);
        assert_eq!(
            eviction_plan(&entries, &opened, &HashSet::new(), 40),
            [entry('b', 40)]
        );
    }

    #[test]
    fn only_hash_named_files_belong_to_the_cache() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let cache = DocumentCache::new(directory.path().join("documents"));
        assert!(cache.entries().expect("lists").is_empty());
        let name = cache_file_name(&hash('a'), "pdf");
        cache.store(&name, b"%PDF-").expect("stores");
        std::fs::create_dir_all(cache.path("0199aa11-scratch")).expect("a scratch folder");
        std::fs::write(cache.path("notes.txt"), b"x").expect("a stray file");
        assert_eq!(cache.entries().expect("lists"), [entry('a', 5)]);
        assert!(cache.contains(&name));
        cache.remove(&name).expect("removes");
        cache.remove(&name).expect("a second removal is harmless");
        assert!(!cache.contains(&name));
    }
}
