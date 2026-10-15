use std::path::{Path, PathBuf};

use sysinfo::Disks;

pub fn free_bytes(path: &Path) -> Result<u64, String> {
    let absolute = std::path::absolute(path).map_err(|error| error.to_string())?;
    let disks = Disks::new_with_refreshed_list();
    let mounts: Vec<(PathBuf, u64)> = disks
        .list()
        .iter()
        .map(|disk| (disk.mount_point().to_path_buf(), disk.available_space()))
        .collect();
    free_on(&mounts, &absolute).ok_or_else(|| format!("No disk holds {}", absolute.display()))
}

pub fn free_on(mounts: &[(PathBuf, u64)], path: &Path) -> Option<u64> {
    mounts
        .iter()
        .filter(|(mount, _)| path.starts_with(mount))
        .max_by_key(|(mount, _)| mount.components().count())
        .map(|(_, free)| *free)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_deepest_mount_holding_the_path_answers() {
        let root = std::env::temp_dir();
        let nested = root.join("volume");
        let mounts = vec![
            (root.clone(), 100),
            (nested.clone(), 7),
            (root.join("elsewhere"), 3),
        ];
        assert_eq!(free_on(&mounts, &nested.join("downloads")), Some(7));
        assert_eq!(free_on(&mounts, &root.join("downloads")), Some(100));
        assert_eq!(free_on(&mounts[1..], &root.join("downloads")), None);
    }

    #[cfg(windows)]
    #[test]
    fn the_real_disk_under_the_temp_directory_has_space() {
        let free = free_bytes(&std::env::temp_dir().join("zvs-missing-folder")).expect("a disk");
        assert!(free > 0);
    }
}
