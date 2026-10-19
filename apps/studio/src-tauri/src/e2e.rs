use std::ffi::OsString;
use std::path::PathBuf;

const WEBVIEW2_PREFIX: &str = "WEBVIEW2_";
#[cfg(any(test, feature = "e2e"))]
const DEFAULT_BROWSER_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection";
#[cfg(feature = "e2e")]
const CDP_PORT: &str = "ZVS_E2E_CDP_PORT";
#[cfg(feature = "e2e")]
const DATA_DIR: &str = "ZVS_DATA_DIR";
const WEBVIEW_DIR: &str = "webview";

pub fn webview2_variables(names: impl IntoIterator<Item = OsString>) -> Vec<OsString> {
    names
        .into_iter()
        .filter(|name| {
            name.to_string_lossy()
                .to_ascii_uppercase()
                .starts_with(WEBVIEW2_PREFIX)
        })
        .collect()
}

#[allow(unsafe_code)]
pub fn clear_webview2_variables_before_any_thread() {
    for name in webview2_variables(std::env::vars_os().map(|(name, _)| name)) {
        unsafe { std::env::remove_var(name) };
    }
}

#[cfg(any(test, feature = "e2e"))]
fn debugging_args(port: Option<&str>) -> Option<String> {
    let port = port?.trim().parse::<u16>().ok().filter(|port| *port != 0)?;
    Some(format!(
        "{DEFAULT_BROWSER_ARGS} --remote-debugging-port={port}"
    ))
}

#[cfg(any(test, feature = "e2e"))]
fn isolated_dir(value: Option<OsString>) -> Option<PathBuf> {
    let path = PathBuf::from(value?);
    path.is_absolute().then_some(path)
}

#[cfg(feature = "e2e")]
pub fn data_dir() -> Option<PathBuf> {
    isolated_dir(std::env::var_os(DATA_DIR))
}

#[cfg(not(feature = "e2e"))]
pub const fn data_dir() -> Option<PathBuf> {
    None
}

#[cfg(feature = "e2e")]
pub fn browser_args() -> Option<String> {
    data_dir()?;
    debugging_args(std::env::var(CDP_PORT).ok().as_deref())
}

#[cfg(not(feature = "e2e"))]
pub const fn browser_args() -> Option<String> {
    None
}

pub fn webview_dir() -> Option<PathBuf> {
    data_dir().map(|dir| dir.join(WEBVIEW_DIR))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_webview2_variable_is_cleared_whatever_its_case() {
        let names = [
            "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
            "webview2_user_data_folder",
            "WebView2_Browser_Executable_Folder",
            "PATH",
            "ZVS_E2E_CDP_PORT",
            "MY_WEBVIEW2_NOTE",
        ]
        .map(OsString::from);
        assert_eq!(
            webview2_variables(names),
            [
                "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
                "webview2_user_data_folder",
                "WebView2_Browser_Executable_Folder",
            ]
            .map(OsString::from)
        );
    }

    #[test]
    fn the_debugging_port_keeps_the_default_arguments() {
        assert_eq!(
            debugging_args(Some(" 9222 ")).as_deref(),
            Some(
                "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --remote-debugging-port=9222"
            )
        );
        for invalid in [None, Some(""), Some("0"), Some("65536"), Some("9222 --x")] {
            assert_eq!(debugging_args(invalid), None);
        }
    }

    #[test]
    fn only_an_absolute_data_dir_isolates_the_app() {
        let absolute = std::env::temp_dir().join("studio-e2e");
        assert_eq!(
            isolated_dir(Some(absolute.clone().into_os_string())),
            Some(absolute)
        );
        assert_eq!(isolated_dir(Some(OsString::from("relative/dir"))), None);
        assert_eq!(isolated_dir(None), None);
    }

    #[cfg(not(feature = "e2e"))]
    #[test]
    fn without_the_feature_nothing_opens_a_port_or_moves_the_data() {
        assert_eq!(browser_args(), None);
        assert_eq!(data_dir(), None);
        assert_eq!(webview_dir(), None);
    }
}
