use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};

use tokio_util::sync::CancellationToken;
use zvs_contract::AppError;

use crate::indexing::patterns::{matches_anchored, matches_pattern, normalize_relative};
use crate::indexing::walk::locale_order;
use crate::kernel::types::cancelled;
use crate::platform::long_path::extended;

pub const GIT_FOLDER: &str = ".git";
pub const IGNORE_FILE: &str = ".gitignore";

#[derive(Debug, Clone, PartialEq, Eq)]
struct Rule {
    pattern: String,
    negate: bool,
    folder_only: bool,
    anchored: bool,
}

fn parse_rules(text: &str) -> Vec<Rule> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim_end();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (negate, line) = line
                .strip_prefix('!')
                .map_or((false, line), |rest| (true, rest));
            let line = line.strip_prefix('\\').unwrap_or(line);
            let (folder_only, line) = line
                .strip_suffix('/')
                .map_or((false, line), |rest| (true, rest));
            let anchored = line.contains('/');
            let pattern = line.trim_start_matches('/').to_owned();
            (!pattern.is_empty()).then_some(Rule {
                pattern,
                negate,
                folder_only,
                anchored,
            })
        })
        .collect()
}

#[derive(Debug, Default)]
pub struct IgnoreRules {
    frames: HashMap<PathBuf, Vec<Rule>>,
}

impl IgnoreRules {
    pub fn for_root(root: &Path) -> Self {
        let mut rules = Self::default();
        let top = root
            .ancestors()
            .find(|ancestor| ancestor.join(GIT_FOLDER).exists());
        match top {
            Some(top) => {
                for ancestor in root.ancestors() {
                    rules.load(ancestor);
                    if ancestor == top {
                        break;
                    }
                }
            }
            None => rules.load(root),
        }
        rules
    }

    pub fn load(&mut self, folder: &Path) {
        if self.frames.contains_key(folder) {
            return;
        }
        if let Ok(text) = std::fs::read_to_string(extended(&folder.join(IGNORE_FILE))) {
            let parsed = parse_rules(&text);
            if !parsed.is_empty() {
                self.frames.insert(folder.to_path_buf(), parsed);
            }
        }
    }

    pub fn ignored(&self, path: &Path, folder: bool) -> bool {
        if folder
            && path
                .file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case(GIT_FOLDER))
        {
            return true;
        }
        let mut frames: Vec<&Path> = path
            .ancestors()
            .skip(1)
            .filter(|ancestor| self.frames.contains_key(*ancestor))
            .collect();
        frames.reverse();
        let mut verdict = false;
        for base in frames {
            let Ok(relative) = path.strip_prefix(base) else {
                continue;
            };
            let relative = normalize_relative(&relative.to_string_lossy());
            for rule in &self.frames[base] {
                if rule.folder_only && !folder {
                    continue;
                }
                let hit = if rule.anchored {
                    matches_anchored(&relative, &rule.pattern)
                } else {
                    matches_pattern(&relative, &rule.pattern)
                };
                if hit {
                    verdict = !rule.negate;
                }
            }
        }
        verdict
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub path: PathBuf,
    pub relative: String,
    pub folder: bool,
    pub link: bool,
    pub bytes: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Listing {
    pub entries: Vec<Entry>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct WalkOptions<'a> {
    pub recursive: bool,
    pub folders: bool,
    pub glob: Option<&'a str>,
    pub limit: usize,
}

pub fn walk(
    root: &Path,
    options: WalkOptions<'_>,
    cancel: &CancellationToken,
) -> Result<Listing, AppError> {
    let mut rules = IgnoreRules::for_root(root);
    let mut listing = Listing::default();
    let mut queue = VecDeque::from([root.to_path_buf()]);
    while let Some(folder) = queue.pop_front() {
        if cancel.is_cancelled() {
            return Err(cancelled());
        }
        rules.load(&folder);
        let Ok(children) = std::fs::read_dir(extended(&folder)) else {
            continue;
        };
        let mut children: Vec<_> = children.filter_map(Result::ok).collect();
        children.sort_by(|left, right| {
            locale_order(
                &left.file_name().to_string_lossy(),
                &right.file_name().to_string_lossy(),
            )
        });
        for child in children {
            let Ok(kind) = child.file_type() else {
                continue;
            };
            let path = folder.join(child.file_name());
            let link = kind.is_symlink();
            let is_folder = kind.is_dir();
            if rules.ignored(&path, is_folder) {
                continue;
            }
            let relative = path
                .strip_prefix(root)
                .map(|relative| normalize_relative(&relative.to_string_lossy()))
                .unwrap_or_default();
            if is_folder {
                if options.recursive && !link {
                    queue.push_back(path.clone());
                }
                if !options.folders {
                    continue;
                }
            } else if options
                .glob
                .is_some_and(|glob| !matches_pattern(&relative, glob))
            {
                continue;
            }
            if listing.entries.len() >= options.limit {
                listing.truncated = true;
                return Ok(listing);
            }
            let bytes = if is_folder {
                0
            } else {
                child.metadata().map_or(0, |metadata| metadata.len())
            };
            listing.entries.push(Entry {
                path,
                relative,
                folder: is_folder,
                link,
                bytes,
            });
        }
    }
    Ok(listing)
}
