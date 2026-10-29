const SEPARATOR: char = ';';

fn comparable(entry: &str) -> String {
    entry
        .trim()
        .trim_matches('"')
        .trim_end_matches(['\\', '/'])
        .replace('/', "\\")
        .to_lowercase()
}

pub fn path_contains(list: &str, folder: &str) -> bool {
    let wanted = comparable(folder);
    !wanted.is_empty()
        && list
            .split(SEPARATOR)
            .any(|entry| comparable(entry) == wanted)
}

pub fn path_with(list: &str, folder: &str) -> Option<String> {
    if path_contains(list, folder) {
        return None;
    }
    let kept = list.trim_end_matches(SEPARATOR);
    Some(if kept.is_empty() {
        folder.to_owned()
    } else {
        format!("{kept}{SEPARATOR}{folder}")
    })
}

pub fn path_without(list: &str, folder: &str) -> Option<String> {
    if !path_contains(list, folder) {
        return None;
    }
    let wanted = comparable(folder);
    Some(
        list.split(SEPARATOR)
            .filter(|entry| comparable(entry) != wanted)
            .collect::<Vec<_>>()
            .join(&SEPARATOR.to_string()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const BIN: &str = r"C:\Users\me\AppData\Local\ZVS AI Studio\cli";

    #[test]
    fn a_folder_is_appended_once() {
        let list = r"C:\Windows;C:\tools;";
        let added = path_with(list, BIN).expect("added");
        assert_eq!(added, format!(r"C:\Windows;C:\tools;{BIN}"));
        assert_eq!(path_with(&added, BIN), None);
        assert_eq!(path_with("", BIN).as_deref(), Some(BIN));
    }

    #[test]
    fn matching_ignores_case_quotes_and_a_trailing_slash() {
        let list = r#"C:\Windows;"c:\users\me\appdata\local\zvs ai studio\cli\""#;
        assert!(path_contains(list, BIN));
        assert!(!path_contains(
            list,
            r"C:\Users\me\AppData\Local\ZVS AI Studio"
        ));
        assert!(!path_contains(list, ""));
    }

    #[test]
    fn removal_keeps_every_other_entry_in_order() {
        let list = format!(r"C:\Windows;{BIN};%USERPROFILE%\bin;{BIN}\");
        assert_eq!(
            path_without(&list, BIN).as_deref(),
            Some(r"C:\Windows;%USERPROFILE%\bin")
        );
        assert_eq!(path_without(r"C:\Windows", BIN), None);
    }
}
