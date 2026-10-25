#![allow(unsafe_code)]

use std::path::PathBuf;

use ::windows::Win32::Foundation::ERROR_SUCCESS;
use ::windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ,
    RegCloseKey, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW,
};
use ::windows::core::{HSTRING, PCWSTR, PWSTR};
use zvs_host::platform::external::InstalledBrowser;

const CLIENTS: &str = r"SOFTWARE\Clients\StartMenuInternet";
const OPEN_COMMAND: &str = r"shell\open\command";
const NAME_UNITS: usize = 256;
const RETIRED: &[&str] = &["IEXPLORE.EXE"];

pub fn program_of(command: &str) -> Option<PathBuf> {
    let text = command.trim();
    let program = if let Some(quoted) = text.strip_prefix('"') {
        quoted.split('"').next()?
    } else {
        let lower = text.to_ascii_lowercase();
        let end = lower.find(".exe").map_or(text.len(), |at| at + 4);
        &text[..end]
    };
    let program = program.trim();
    (!program.is_empty()).then(|| PathBuf::from(program))
}

pub fn distinct(found: Vec<InstalledBrowser>) -> Vec<InstalledBrowser> {
    let mut seen: Vec<String> = Vec::new();
    let mut browsers: Vec<InstalledBrowser> = Vec::new();
    for browser in found {
        let key = browser.program.to_string_lossy().to_lowercase();
        let retired = RETIRED.iter().any(|id| browser.id.eq_ignore_ascii_case(id));
        if retired || seen.contains(&key) || browsers.iter().any(|known| known.id == browser.id) {
            continue;
        }
        seen.push(key);
        browsers.push(browser);
    }
    browsers.sort_by_key(|browser| browser.name.to_lowercase());
    browsers
}

fn read_string(root: HKEY, subkey: &str) -> Option<String> {
    let subkey = HSTRING::from(subkey);
    let flags = RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ;
    let mut bytes = 0_u32;
    let status = unsafe {
        RegGetValueW(
            root,
            &subkey,
            PCWSTR::null(),
            flags,
            None,
            None,
            Some(&raw mut bytes),
        )
    };
    if status != ERROR_SUCCESS || bytes == 0 {
        return None;
    }
    let mut buffer = vec![0_u16; (bytes as usize).div_ceil(2)];
    let status = unsafe {
        RegGetValueW(
            root,
            &subkey,
            PCWSTR::null(),
            flags,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&raw mut bytes),
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let end = buffer
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(buffer.len());
    let text = String::from_utf16_lossy(&buffer[..end]);
    (!text.trim().is_empty()).then(|| text.trim().to_owned())
}

fn client_keys(root: HKEY) -> Vec<String> {
    let mut key = HKEY::default();
    let opened =
        unsafe { RegOpenKeyExW(root, &HSTRING::from(CLIENTS), None, KEY_READ, &raw mut key) };
    if opened != ERROR_SUCCESS {
        return Vec::new();
    }
    let mut names = Vec::new();
    for index in 0.. {
        let mut buffer = [0_u16; NAME_UNITS];
        let mut length = NAME_UNITS as u32;
        let status = unsafe {
            RegEnumKeyExW(
                key,
                index,
                Some(PWSTR(buffer.as_mut_ptr())),
                &raw mut length,
                None,
                None,
                None,
                None,
            )
        };
        if status != ERROR_SUCCESS {
            break;
        }
        names.push(String::from_utf16_lossy(&buffer[..length as usize]));
    }
    let _ = unsafe { RegCloseKey(key) };
    names
}

pub fn installed() -> Vec<InstalledBrowser> {
    let mut found = Vec::new();
    for root in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        for id in client_keys(root) {
            let base = format!(r"{CLIENTS}\{id}");
            let Some(program) = read_string(root, &format!(r"{base}\{OPEN_COMMAND}"))
                .as_deref()
                .and_then(program_of)
                .filter(|program| program.is_file())
            else {
                continue;
            };
            let name = read_string(root, &base).unwrap_or_else(|| id.clone());
            found.push(InstalledBrowser { id, name, program });
        }
    }
    distinct(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_program_is_read_from_quoted_and_bare_commands() {
        assert_eq!(
            program_of(r#""C:\Program Files\Google\Chrome\Application\chrome.exe""#),
            Some(PathBuf::from(
                r"C:\Program Files\Google\Chrome\Application\chrome.exe"
            ))
        );
        assert_eq!(
            program_of(r#""C:\Program Files\Mozilla Firefox\firefox.exe" -osint -url "%1""#),
            Some(PathBuf::from(
                r"C:\Program Files\Mozilla Firefox\firefox.exe"
            ))
        );
        assert_eq!(
            program_of(r"C:\Opera\launcher.exe --single-argument %1"),
            Some(PathBuf::from(r"C:\Opera\launcher.exe"))
        );
        assert_eq!(program_of("  "), None);
    }

    #[test]
    fn a_browser_registered_twice_is_listed_once_and_by_name() {
        let browser = |id: &str, name: &str, program: &str| InstalledBrowser {
            id: id.to_owned(),
            name: name.to_owned(),
            program: PathBuf::from(program),
        };
        let listed = distinct(vec![
            browser("Microsoft Edge", "Microsoft Edge", r"C:\Edge\msedge.exe"),
            browser("Google Chrome", "Google Chrome", r"C:\Chrome\chrome.exe"),
            browser(
                "Google Chrome.ABC",
                "Google Chrome",
                r"C:\CHROME\chrome.exe",
            ),
            browser("IEXPLORE.EXE", "Internet Explorer", r"C:\IE\iexplore.exe"),
        ]);
        assert_eq!(
            listed
                .iter()
                .map(|browser| browser.id.as_str())
                .collect::<Vec<_>>(),
            ["Google Chrome", "Microsoft Edge"]
        );
    }

    #[test]
    fn the_registry_lists_only_programs_that_exist() {
        for browser in installed() {
            assert!(browser.program.is_file(), "{}", browser.program.display());
            assert!(!browser.name.is_empty());
        }
    }
}
