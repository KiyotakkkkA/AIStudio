use std::sync::LazyLock;

use regex::Regex;

pub const APP_NAME_MAX: usize = 128;

pub const FORBIDDEN_APPS: &[&str] = &[
    "consent.exe",
    "credentialuibroker.exe",
    "logonui.exe",
    "lockapp.exe",
    "keepass.exe",
    "keepassxc.exe",
    "1password.exe",
    "bitwarden.exe",
    "dashlane.exe",
    "lastpass.exe",
    "enpass.exe",
    "proton pass.exe",
];

pub const SENSITIVE_APPS: &[&str] = &[
    "cmd.exe",
    "powershell.exe",
    "powershell_ise.exe",
    "pwsh.exe",
    "windowsterminal.exe",
    "wt.exe",
    "openconsole.exe",
    "conhost.exe",
    "wsl.exe",
    "wslhost.exe",
    "bash.exe",
    "mintty.exe",
    "regedit.exe",
    "mmc.exe",
    "taskmgr.exe",
    "systemsettings.exe",
    "control.exe",
    "msiexec.exe",
    "rundll32.exe",
    "devenv.exe",
];

static APP_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[\p{L}\p{N} ._()+\-]+$").expect("a valid pattern"));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppClass {
    Forbidden,
    Sensitive,
    Normal,
}

pub fn normalize_app(raw: &str) -> Option<String> {
    let name = raw.trim().rsplit(['\\', '/']).next()?.trim().to_lowercase();
    if name.is_empty() || name.chars().count() > APP_NAME_MAX || !APP_NAME.is_match(&name) {
        return None;
    }
    if name.ends_with(".exe") {
        Some(name)
    } else {
        Some(format!("{name}.exe"))
    }
}

pub fn classify(app: &str) -> AppClass {
    if FORBIDDEN_APPS.contains(&app) {
        AppClass::Forbidden
    } else if SENSITIVE_APPS.contains(&app) {
        AppClass::Sensitive
    } else {
        AppClass::Normal
    }
}

fn key_names(combo: &str) -> Vec<String> {
    let text: String = combo.chars().filter(|char| !char.is_whitespace()).collect();
    let mut names: Vec<String> = text
        .split('+')
        .filter(|part| !part.is_empty())
        .map(str::to_lowercase)
        .collect();
    if text.ends_with('+') {
        names.push("+".to_owned());
    }
    names
}

pub fn sensitive_keys(combo: &str) -> bool {
    let names = key_names(combo);
    let has = |options: &[&str]| names.iter().any(|name| options.contains(&name.as_str()));
    let win = has(&["win", "meta", "super", "cmd", "lwin", "rwin"]);
    let ctrl = has(&["ctrl", "control"]);
    let alt = has(&["alt"]);
    let shift = has(&["shift"]);
    win || (ctrl && alt && has(&["delete", "del", "end"]))
        || (ctrl && shift && has(&["esc", "escape"]))
}

pub fn valid_combo(combo: &str) -> bool {
    let names = key_names(combo);
    !names.is_empty() && names.len() <= 5 && names.iter().all(|name| name.chars().count() <= 16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_names_are_lowercase_executable_basenames() {
        assert_eq!(normalize_app("Notepad").as_deref(), Some("notepad.exe"));
        assert_eq!(
            normalize_app(r"C:\Program Files\Google\Chrome\Application\chrome.exe").as_deref(),
            Some("chrome.exe")
        );
        assert_eq!(normalize_app(" EXCEL.EXE ").as_deref(), Some("excel.exe"));
        assert_eq!(normalize_app("").as_deref(), None);
        assert_eq!(normalize_app("evil;rm").as_deref(), None);
        assert_eq!(normalize_app("a*b").as_deref(), None);
    }

    #[test]
    fn credential_prompts_are_forbidden_and_terminals_are_sensitive() {
        assert_eq!(classify("consent.exe"), AppClass::Forbidden);
        assert_eq!(classify("keepassxc.exe"), AppClass::Forbidden);
        assert_eq!(classify("pwsh.exe"), AppClass::Sensitive);
        assert_eq!(classify("windowsterminal.exe"), AppClass::Sensitive);
        assert_eq!(classify("notepad.exe"), AppClass::Normal);
    }

    #[test]
    fn system_shortcuts_are_sensitive_and_editing_shortcuts_are_not() {
        assert!(sensitive_keys("win+r"));
        assert!(sensitive_keys("Ctrl + Alt + Delete"));
        assert!(sensitive_keys("ctrl+shift+esc"));
        assert!(!sensitive_keys("ctrl+s"));
        assert!(!sensitive_keys("alt+tab"));
        assert!(!sensitive_keys("ctrl++"));
    }

    #[test]
    fn combos_are_bounded() {
        assert!(valid_combo("ctrl+shift+t"));
        assert!(valid_combo("+"));
        assert!(!valid_combo(""));
        assert!(!valid_combo("a+b+c+d+e+f"));
    }
}
