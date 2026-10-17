use std::path::{Path, PathBuf};

pub const EXTENDED: &str = r"\\?\";
pub const EXTENDED_UNC: &str = r"\\?\UNC\";

fn is_windows_absolute(path: &str) -> bool {
    let bytes = path.as_bytes();
    let separator = |byte: u8| byte == b'\\' || byte == b'/';
    match bytes {
        [first, ..] if separator(*first) => true,
        [drive, b':', slash, ..] => drive.is_ascii_alphabetic() && separator(*slash),
        _ => false,
    }
}

pub fn to_extended_path(path: &str, windows: bool) -> String {
    if !windows || path.starts_with(EXTENDED) || !is_windows_absolute(path) {
        return path.to_owned();
    }
    let backslashed = path.replace('/', "\\");
    match backslashed.strip_prefix(r"\\") {
        Some(rest) => format!("{EXTENDED_UNC}{rest}"),
        None => format!("{EXTENDED}{backslashed}"),
    }
}

pub fn from_extended_path(path: &str) -> String {
    if let Some(rest) = path.strip_prefix(EXTENDED_UNC) {
        return format!(r"\\{rest}");
    }
    path.strip_prefix(EXTENDED).unwrap_or(path).to_owned()
}

pub fn extended(path: &Path) -> PathBuf {
    PathBuf::from(to_extended_path(&path.to_string_lossy(), cfg!(windows)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::temporary_directory;

    #[test]
    fn a_drive_path_takes_the_prefix_and_a_unc_path_takes_its_own_spelling() {
        assert_eq!(
            to_extended_path(r"C:\docs\a.txt", true),
            r"\\?\C:\docs\a.txt"
        );
        assert_eq!(
            to_extended_path("C:/docs/a.txt", true),
            r"\\?\C:\docs\a.txt"
        );
        assert_eq!(
            to_extended_path(r"\\server\share\a.txt", true),
            r"\\?\UNC\server\share\a.txt"
        );
    }

    #[test]
    fn nothing_is_prefixed_twice_relatively_or_off_windows() {
        assert_eq!(to_extended_path(r"\\?\C:\a.txt", true), r"\\?\C:\a.txt");
        assert_eq!(to_extended_path(r"docs\a.txt", true), r"docs\a.txt");
        assert_eq!(to_extended_path("C:a.txt", true), "C:a.txt");
        assert_eq!(
            to_extended_path("/home/kirza/a.txt", false),
            "/home/kirza/a.txt"
        );
    }

    #[test]
    fn the_prefix_round_trips_off_for_storage_or_the_screen() {
        assert_eq!(
            from_extended_path(&to_extended_path(r"C:\docs\a.txt", true)),
            r"C:\docs\a.txt"
        );
        assert_eq!(
            from_extended_path(&to_extended_path(r"\\server\share\a.txt", true)),
            r"\\server\share\a.txt"
        );
        assert_eq!(from_extended_path(r"C:\docs\a.txt"), r"C:\docs\a.txt");
    }

    fn deep_directory(root: &Path) -> PathBuf {
        let mut path = root.to_path_buf();
        while path.to_string_lossy().chars().count() < 300 {
            path = path.join("взыскание денежных средств (ИП Гусаров)");
        }
        path
    }

    #[test]
    fn a_path_past_the_260_character_limit_is_written_and_read_with_or_without_the_prefix() {
        let workspace = temporary_directory("longpath-");
        let deep = deep_directory(workspace.path());
        std::fs::create_dir_all(extended(&deep)).expect("creates the tree");
        let file = deep.join("определение суда от 14.03.2026.txt");
        assert!(file.to_string_lossy().chars().count() > 260);
        std::fs::write(extended(&file), "спорная сумма взыскана").expect("writes");
        assert_eq!(
            std::fs::read_to_string(&file).expect("std reads the plain long path"),
            "спорная сумма взыскана"
        );
        assert_eq!(
            std::fs::read_to_string(extended(&file)).expect("reads through the prefix"),
            "спорная сумма взыскана"
        );
        if cfg!(windows) {
            assert!(extended(&file).to_string_lossy().starts_with(EXTENDED));
        }
        assert!(!from_extended_path(&extended(&file).to_string_lossy()).starts_with(EXTENDED));
    }
}
