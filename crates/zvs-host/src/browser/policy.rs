use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use url::Url;
use zvs_contract::validate::js_trim;

pub const BROWSER_PARTITION: &str = "persist:browser-work";
pub const PROFILE_DIR: [&str; 2] = ["browser", "work"];
pub const CHROME_HEIGHT: f64 = 148.0;
pub const STATUS_HEIGHT: f64 = 28.0;
pub const DEVTOOLS_SHARE: f64 = 0.45;
pub const AGENT_PANEL_WIDTH: f64 = 344.0;
pub const APPROVAL_HEIGHT: f64 = 76.0;
pub const TAB_LIMIT: usize = 30;
pub const ONLY_WEB_ADDRESSES: &str = "Only HTTP and HTTPS addresses are allowed";
pub const NAVIGATION_BLOCKED: &str = "Navigation blocked: only HTTP/HTTPS is allowed";

static SCHEME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^[a-z][a-z\d+.-]*:").expect("a static pattern"));
static HOST_WITH_PORT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[\w.-]+:\d+(?:/|$)").expect("a static pattern"));

pub fn profile_dir(data_dir: &Path) -> PathBuf {
    PROFILE_DIR
        .iter()
        .fold(data_dir.to_path_buf(), |path, part| path.join(part))
}

pub fn devtools_height(content: f64) -> f64 {
    (content.max(0.0) * DEVTOOLS_SHARE).round()
}

fn parse_web(value: &str) -> Option<Url> {
    let url = Url::parse(value).ok()?;
    let web = matches!(url.scheme(), "http" | "https")
        && url.username().is_empty()
        && url.password().is_none();
    web.then_some(url)
}

pub fn is_web_url(value: &str) -> bool {
    parse_web(value).is_some()
}

pub fn navigation_url(input: &str) -> Result<String, &'static str> {
    let text = js_trim(input);
    let candidate = if SCHEME.is_match(text) && !HOST_WITH_PORT.is_match(text) {
        text.to_owned()
    } else {
        format!("https://{text}")
    };
    parse_web(&candidate)
        .map(String::from)
        .ok_or(ONLY_WEB_ADDRESSES)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_sites_are_normalised_and_local_http_addresses_kept() {
        assert_eq!(
            navigation_url(" example.com/path "),
            Ok("https://example.com/path".to_owned())
        );
        assert_eq!(
            navigation_url("http://localhost:8123/a"),
            Ok("http://localhost:8123/a".to_owned())
        );
        assert_eq!(
            navigation_url("localhost:8123/a"),
            Ok("https://localhost:8123/a".to_owned())
        );
        assert_eq!(
            navigation_url("HTTPS://Example.COM"),
            Ok("https://example.com/".to_owned())
        );
    }

    #[test]
    fn non_web_addresses_are_rejected() {
        for url in [
            "file:///etc/passwd",
            "file:///C:/",
            "javascript:alert(1)",
            "data:text/html,test",
            "about:blank",
            "ftp://example.com",
            "https://user:password@example.com",
            "https://user@example.com",
            "not a url",
            "",
        ] {
            assert!(!is_web_url(url), "{url}");
            assert_eq!(navigation_url(url), Err(ONLY_WEB_ADDRESSES), "{url}");
        }
    }

    #[test]
    fn web_urls_are_http_or_https_without_credentials() {
        assert!(is_web_url("https://example.com/"));
        assert!(is_web_url("http://127.0.0.1:8080/a?b#c"));
        assert!(!is_web_url("example.com"));
    }

    #[test]
    fn devtools_take_a_rounded_share_of_the_content_area() {
        assert_eq!(devtools_height(700.0), 315.0);
        assert_eq!(devtools_height(101.0), 45.0);
        assert_eq!(devtools_height(-5.0), 0.0);
    }

    #[test]
    fn the_profile_lives_under_browser_work() {
        let dir = profile_dir(Path::new("data"));
        assert_eq!(dir, Path::new("data").join("browser").join("work"));
    }
}
