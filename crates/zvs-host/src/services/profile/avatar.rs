use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{self, Read, Write as _};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use uuid::Uuid;
use zvs_contract::{AppError, AppErrorCode, PROFILE_AVATAR_MAX, ProfileAvatarDto};

const FILE_NAME: &str = "profile-avatar.svg";
const SVG_START: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 9 9">"#;

pub struct ProfileAvatarService {
    cache_dir: PathBuf,
    access: Arc<Mutex<()>>,
}

impl ProfileAvatarService {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self {
            cache_dir,
            access: Arc::new(Mutex::new(())),
        }
    }

    pub async fn load(&self) -> Result<ProfileAvatarDto, AppError> {
        let access = Arc::clone(&self.access);
        let cache_dir = self.cache_dir.clone();
        let svg = tokio::task::spawn_blocking(move || {
            let _access = access.lock().unwrap_or_else(PoisonError::into_inner);
            load_or_generate(&cache_dir)
        })
        .await
        .map_err(native)?
        .map_err(native)?;
        Ok(ProfileAvatarDto { svg })
    }
}

fn native(error: impl std::fmt::Display) -> AppError {
    AppError::new(AppErrorCode::NativeError, error.to_string())
}

fn load_or_generate(cache_dir: &Path) -> io::Result<String> {
    let path = cache_dir.join(FILE_NAME);
    match File::open(&path) {
        Ok(file) => {
            let mut bytes = Vec::new();
            file.take((PROFILE_AVATAR_MAX + 1) as u64)
                .read_to_end(&mut bytes)?;
            if bytes.len() <= PROFILE_AVATAR_MAX
                && let Ok(svg) = String::from_utf8(bytes)
                && svg.starts_with(SVG_START)
                && svg.ends_with("</svg>")
            {
                return Ok(svg);
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }

    let svg = generate(Uuid::new_v4());
    fs::create_dir_all(cache_dir)?;
    let mut temporary = tempfile::NamedTempFile::new_in(cache_dir)?;
    temporary.write_all(svg.as_bytes())?;
    temporary.as_file().sync_all()?;
    temporary.persist(path)?;
    Ok(svg)
}

fn generate(seed: Uuid) -> String {
    let bytes = seed.as_bytes();
    let hue = u16::from_be_bytes([bytes[0], bytes[1]]) % 360;
    let mut svg = format!(
        r#"{SVG_START}<path fill="hsl({hue},48%,22%)" d="M0 0h9v9H0z"/><path fill="hsl({hue},76%,75%)" d=""#
    );
    // Mirror four columns around the center of a seven-cell grid.
    for (row, byte) in bytes[2..9].iter().copied().enumerate() {
        let y = row + 1;
        for column in 0..4 {
            if byte & (1 << column) != 0 || (row == 3 && column == 3) {
                let x = column + 1;
                write!(svg, "M{x} {y}h1v1H{x}z").expect("writing to String");
                if column != 3 {
                    let mirror = 8 - x;
                    write!(svg, "M{mirror} {y}h1v1H{mirror}z").expect("writing to String");
                }
            }
        }
    }
    svg.push_str(r#""/></svg>"#);
    svg
}

#[cfg(test)]
mod tests {
    use super::*;
    use zvs_contract::Validate;

    #[tokio::test]
    async fn creates_one_avatar_and_reuses_it_across_service_instances() {
        let root = tempfile::tempdir().expect("temporary state");
        let cache = root.path().join("cache");
        let service = ProfileAvatarService::new(cache.clone());
        let (first, concurrent) = tokio::join!(service.load(), service.load());
        let first = first.expect("generated avatar");
        first.validate().expect("valid avatar payload");
        assert_eq!(concurrent.expect("concurrent load"), first);
        assert_eq!(
            fs::read_to_string(cache.join(FILE_NAME)).unwrap(),
            first.svg
        );

        let restarted = ProfileAvatarService::new(cache);
        assert_eq!(restarted.load().await.expect("cached avatar"), first);
    }

    #[test]
    fn repairs_an_invalid_cache_entry() {
        let cache = tempfile::tempdir().expect("temporary cache");
        let path = cache.path().join(FILE_NAME);
        fs::write(&path, [0xff]).expect("invalid cached bytes");
        let svg = load_or_generate(cache.path()).expect("replacement avatar");
        assert!(svg.starts_with(SVG_START));
        assert_eq!(fs::read_to_string(path).unwrap(), svg);
    }
}
