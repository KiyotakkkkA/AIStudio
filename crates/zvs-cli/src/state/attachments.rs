use std::path::{Path, PathBuf};

use zvs_contract::VectorStoreId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Attachment {
    Store { id: VectorStoreId, name: String },
    Image { path: String, name: String },
}

impl Attachment {
    pub fn name(&self) -> &str {
        match self {
            Self::Store { name, .. } | Self::Image { name, .. } => name,
        }
    }

    pub const fn image(&self) -> bool {
        matches!(self, Self::Image { .. })
    }

    pub const fn icon(&self) -> &'static str {
        match self {
            Self::Store { .. } => "▤",
            Self::Image { .. } => "▧",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub start: usize,
    pub end: usize,
    pub attachment: Attachment,
}

pub fn image_path(text: &str) -> Option<Attachment> {
    let text = text.trim();
    let text = text
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(text);
    let path = Path::new(text);
    let bytes = text.as_bytes();
    let windows_absolute = text.starts_with(r"\\")
        || (bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'/' | b'\\'));
    if !(path.is_absolute() || windows_absolute) || !is_image(path) {
        return None;
    }
    Some(Attachment::Image {
        path: text.to_owned(),
        name: text.rsplit(['/', '\\']).next()?.to_owned(),
    })
}

fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| {
            ["png", "jpg", "jpeg", "gif", "webp"].contains(&ext.to_ascii_lowercase().as_str())
        })
}

/// Do not follow links or junctions outside the selected working folder.
pub fn discover(folder: &str) -> Vec<Attachment> {
    let Ok(root) = std::fs::canonicalize(PathBuf::from(folder)) else {
        return Vec::new();
    };
    let mut pending = vec![root.clone()];
    let mut images = Vec::new();
    let mut visited = std::collections::BTreeSet::new();
    while let Some(dir) = pending.pop() {
        if !visited.insert(dir.clone()) {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if kind.is_symlink()
                || std::fs::canonicalize(&path).is_ok_and(|p| !p.starts_with(&root))
            {
                continue;
            }
            if kind.is_dir() {
                if let Ok(path) = std::fs::canonicalize(path) {
                    pending.push(path);
                }
            } else if kind.is_file() && is_image(&path) {
                let name = path
                    .strip_prefix(&root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .into_owned();
                images.push(Attachment::Image {
                    path: path.to_string_lossy().into_owned(),
                    name,
                });
            }
        }
    }
    images.sort_by(|a, b| a.name().cmp(b.name()));
    images
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn images_are_discovered_recursively_including_uppercase_extensions() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("nested")).unwrap();
        for file in ["one.PNG", "nested/two.webp", "text.txt"] {
            std::fs::write(root.path().join(file), b"fixture").unwrap();
        }
        let images = discover(root.path().to_str().unwrap());
        assert_eq!(images.len(), 2);
        assert!(images.iter().any(|a| a.name() == "one.PNG"));
        assert!(images.iter().any(|a| a.name().ends_with("two.webp")));
    }
}
