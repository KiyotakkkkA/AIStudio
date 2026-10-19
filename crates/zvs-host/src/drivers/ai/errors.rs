use std::collections::BTreeMap;
use std::error::Error as StdError;
use std::io;
use std::sync::LazyLock;
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;
use serde_json::{Value, json};
use zvs_contract::{AppError, AppErrorCode, AuthMode, ErrorDetails};

pub type Headers = BTreeMap<String, String>;

pub const CANCELLED_MESSAGE: &str = "Запрос к провайдеру отменён";
pub const TIMED_OUT_MESSAGE: &str = "Провайдер не ответил вовремя";
pub const UNREACHABLE_MESSAGE: &str = "Провайдер недоступен";
pub const REQUEST_FAILED_MESSAGE: &str = "Не удалось выполнить запрос к провайдеру";
pub const SESSION_EXPIRED_MESSAGE: &str = "Сессия аккаунта истекла — требуется повторная привязка";
pub const BROWSER_VERIFICATION_MESSAGE: &str = "Провайдер запросил проверку браузера. Откройте страницу аккаунта, завершите проверку и повторите запрос";

const MAX_CAUSE_DEPTH: usize = 8;

static TIMED_OUT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)timed?\s?out").expect("a valid pattern"));

#[derive(Debug, Clone, Default, PartialEq)]
pub struct HttpFailure {
    pub status: u16,
    pub headers: Headers,
    pub body: Option<String>,
    pub details: ErrorDetails,
    pub auth_mode: Option<AuthMode>,
}

impl HttpFailure {
    pub fn status(status: u16) -> Self {
        Self {
            status,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn with_body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into());
        self
    }

    #[must_use]
    pub fn with_header(mut self, name: &str, value: impl Into<String>) -> Self {
        self.headers.insert(name.to_lowercase(), value.into());
        self
    }
}

pub fn js_number(value: f64) -> Value {
    if value.fract() == 0.0 && value.abs() < 9_007_199_254_740_992.0 {
        let whole = value as i64;
        json!(whole)
    } else {
        json!(value)
    }
}

pub fn cancelled() -> AppError {
    AppError::new(AppErrorCode::RunCancelled, CANCELLED_MESSAGE)
}

pub fn timed_out(seconds: f64) -> AppError {
    AppError::new(AppErrorCode::ProviderUnreachable, TIMED_OUT_MESSAGE)
        .with_detail("timeoutSeconds", js_number(seconds))
}

pub fn request_failed() -> AppError {
    AppError::new(AppErrorCode::Unknown, REQUEST_FAILED_MESSAGE)
}

pub fn session_expired(details: ErrorDetails) -> AppError {
    AppError::new(
        AppErrorCode::ProviderSessionExpired,
        SESSION_EXPIRED_MESSAGE,
    )
    .with_details(details)
}

pub fn browser_verification(details: ErrorDetails) -> AppError {
    AppError::new(
        AppErrorCode::ProviderUnreachable,
        BROWSER_VERIFICATION_MESSAGE,
    )
    .with_details(details)
}

pub fn is_html(headers: &Headers) -> bool {
    headers
        .get("content-type")
        .is_some_and(|value| value.to_lowercase().contains("text/html"))
}

pub fn is_session_expired(error: &AppError) -> bool {
    error.code == AppErrorCode::ProviderSessionExpired
}

pub fn is_cancellation(error: &AppError) -> bool {
    error.code == AppErrorCode::RunCancelled
}

pub fn is_not_found_response(error: &AppError) -> bool {
    error
        .details
        .as_ref()
        .and_then(|details| details.get("status"))
        .and_then(Value::as_u64)
        == Some(404)
}

pub fn is_signed_out_response(status: u16, headers: &Headers) -> bool {
    (300..400).contains(&status) || is_html(headers)
}

const MAX_TITLE_CHARS: usize = 120;
const MAX_ENVELOPE_BYTES: usize = 64 * 1024;
const MAX_CODE_CHARS: usize = 64;

/// The code of a JSON error envelope that rejects the bearer token while answering HTTP 2xx,
/// as Qwen does: `{"success": false, "data": {"code": "unauthorized", ...}}`.
pub fn auth_envelope_code(text: &str) -> Option<String> {
    if text.len() > MAX_ENVELOPE_BYTES {
        return None;
    }
    let value = serde_json::from_str::<Value>(text).ok()?;
    if value.get("success") != Some(&Value::Bool(false)) {
        return None;
    }
    let code = match value
        .get("data")
        .and_then(|data| data.get("code"))
        .or_else(|| value.get("code"))?
    {
        Value::String(code) if code.len() <= MAX_CODE_CHARS => code.clone(),
        Value::Number(code) => code.to_string(),
        _ => return None,
    };
    let lower = code.to_ascii_lowercase();
    let rejected = matches!(lower.as_str(), "401" | "403")
        || [
            "unauth",
            "not_login",
            "notlogin",
            "token",
            "expire",
            "forbidden",
        ]
        .iter()
        .any(|needle| lower.contains(needle));
    rejected.then_some(code)
}

/// The `<title>` of an HTML page, which tells a login, verification or error page apart.
pub fn html_title(text: &str) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    let open = lower.find("<title")?;
    let start = open + lower[open..].find('>')? + 1;
    let end = start + lower[start..].find("</title")?;
    let title = text
        .get(start..end)?
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    (!title.is_empty()).then(|| title.chars().take(MAX_TITLE_CHARS).collect())
}

/// Inspect page evidence without exposing HTML or URL query credentials.
pub fn account_page_error(
    status: u16,
    headers: &Headers,
    body: &str,
    final_url: Option<&str>,
    redirected: bool,
    mut details: ErrorDetails,
) -> Option<AppError> {
    if !is_signed_out_response(status, headers) {
        return None;
    }
    details.insert("status".to_owned(), json!(status));
    details.insert("redirected".to_owned(), json!(redirected));
    let destination = if (300..400).contains(&status) {
        headers.get("location").map(String::as_str).or(final_url)
    } else {
        final_url
    };
    let path = destination.and_then(|value| {
        url::Url::parse(value)
            .ok()
            .map(|url| url.path().to_owned())
            .or_else(|| {
                value.starts_with('/').then(|| {
                    value
                        .split(['?', '#'])
                        .next()
                        .unwrap_or_default()
                        .to_owned()
                })
            })
    });
    if let Some(path) = &path {
        details.insert("finalPath".to_owned(), json!(path));
    }
    let prefix =
        String::from_utf8_lossy(&body.as_bytes()[..body.len().min(32 * 1024)]).to_ascii_lowercase();
    let challenge = [
        "cf-chl-",
        "/cdn-cgi/challenge-platform/",
        "id=\"baxia-dialog\"",
        "id='baxia-dialog'",
        "baxia-punish",
        "<title>verify you are human</title>",
        "<title>checking your browser</title>",
        "<title>security verification</title>",
    ]
    .iter()
    .any(|marker| prefix.contains(marker));
    if challenge {
        details.insert("reason".to_owned(), json!("browser-verification"));
        return Some(browser_verification(details));
    }
    let login = path.as_deref().is_some_and(|path| {
        path.split('/').any(|part| {
            matches!(
                part.to_ascii_lowercase().as_str(),
                "login" | "signin" | "sign-in" | "auth"
            )
        })
    }) || [
        "<title>sign in</title>",
        "<title>log in</title>",
        "<title>login</title>",
    ]
    .iter()
    .any(|marker| prefix.contains(marker));
    if login {
        details.insert("reason".to_owned(), json!("login-required"));
        return Some(session_expired(details));
    }
    details.insert(
        "reason".to_owned(),
        json!(if is_html(headers) {
            "unexpected-html"
        } else {
            "unexpected-redirect"
        }),
    );
    Some(
        AppError::new(
            AppErrorCode::ProviderUnreachable,
            if is_html(headers) {
                "Провайдер вернул HTML вместо ответа API"
            } else {
                "Провайдер перенаправил запрос вместо ответа API"
            },
        )
        .with_details(details),
    )
}

pub fn http_failure(failure: HttpFailure) -> AppError {
    http_failure_at(failure, now_ms())
}

pub fn http_failure_at(failure: HttpFailure, now: i64) -> AppError {
    let status = failure.status;
    let mut details = ErrorDetails::new();
    details.insert("status".to_owned(), json!(status));
    details.extend(failure.details);
    if let Some(message) = extract_message(failure.body.as_deref()) {
        details.insert("vendorMessage".to_owned(), json!(message));
    }

    if status == 401 || status == 403 {
        if failure.auth_mode == Some(AuthMode::Account) {
            return session_expired(details);
        }
        return AppError::new(
            AppErrorCode::ProviderAuthFailed,
            "Провайдер отклонил учётные данные",
        )
        .with_details(details);
    }
    if status == 429 {
        if let Some(retry_after) = failure
            .headers
            .get("retry-after")
            .and_then(|header| parse_retry_after(header, now))
        {
            details.insert("retryAfter".to_owned(), json!(retry_after));
        }
        return AppError::new(
            AppErrorCode::RateLimited,
            "Провайдер ограничил частоту запросов",
        )
        .with_details(details);
    }
    if status == 408 || status == 504 {
        return AppError::new(AppErrorCode::ProviderUnreachable, TIMED_OUT_MESSAGE)
            .with_details(details);
    }
    if status >= 500 {
        return AppError::new(
            AppErrorCode::ProviderUnreachable,
            "Провайдер вернул ошибку сервера",
        )
        .with_details(details);
    }
    AppError::new(AppErrorCode::Unknown, "Провайдер вернул неизвестную ошибку")
        .with_details(details)
}

pub fn network_failure(error: &(dyn StdError + 'static)) -> AppError {
    let chain = causes(error);
    if let Some(code) = chain.iter().find_map(|cause| syscall_code(*cause)) {
        return AppError::new(AppErrorCode::ProviderUnreachable, UNREACHABLE_MESSAGE)
            .with_detail("syscall", code);
    }
    if chain
        .iter()
        .any(|cause| TIMED_OUT.is_match(&cause.to_string()))
        || chain.iter().any(|cause| {
            cause
                .downcast_ref::<reqwest::Error>()
                .is_some_and(reqwest::Error::is_timeout)
        })
    {
        return AppError::new(AppErrorCode::ProviderUnreachable, TIMED_OUT_MESSAGE);
    }
    if chain.iter().any(|cause| {
        cause
            .downcast_ref::<reqwest::Error>()
            .is_some_and(reqwest::Error::is_connect)
    }) {
        return AppError::new(AppErrorCode::ProviderUnreachable, UNREACHABLE_MESSAGE);
    }
    request_failed()
}

pub fn raw_error_text(error: &AppError) -> String {
    error
        .details
        .as_ref()
        .and_then(|details| details.get("vendorMessage"))
        .and_then(Value::as_str)
        .map_or_else(|| error.message.clone(), str::to_owned)
}

pub fn raw_cause_text(error: &(dyn StdError + 'static)) -> String {
    causes(error)
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\nCaused by: ")
}

fn causes<'a>(error: &'a (dyn StdError + 'static)) -> Vec<&'a (dyn StdError + 'static)> {
    let mut chain = Vec::new();
    let mut current = Some(error);
    while let Some(cause) = current {
        if chain.len() == MAX_CAUSE_DEPTH {
            break;
        }
        chain.push(cause);
        current = cause.source();
    }
    chain
}

fn syscall_code(error: &(dyn StdError + 'static)) -> Option<&'static str> {
    if let Some(io) = error.downcast_ref::<io::Error>() {
        let code = match io.kind() {
            io::ErrorKind::ConnectionRefused => Some("ECONNREFUSED"),
            io::ErrorKind::ConnectionReset | io::ErrorKind::ConnectionAborted => Some("ECONNRESET"),
            io::ErrorKind::HostUnreachable => Some("EHOSTUNREACH"),
            io::ErrorKind::NetworkUnreachable => Some("ENETUNREACH"),
            io::ErrorKind::BrokenPipe => Some("EPIPE"),
            io::ErrorKind::TimedOut => Some("ETIMEDOUT"),
            io::ErrorKind::UnexpectedEof => Some("UND_ERR_SOCKET"),
            _ => None,
        };
        if code.is_some() {
            return code;
        }
    }
    let message = error.to_string().to_lowercase();
    if message.contains("dns error")
        || message.contains("failed to lookup address")
        || message.contains("no such host is known")
    {
        return Some("ENOTFOUND");
    }
    None
}

fn extract_message(body: Option<&str>) -> Option<String> {
    let body = body?;
    if body.trim().is_empty() {
        return None;
    }
    if let Ok(parsed @ (Value::Object(_) | Value::Array(_))) = serde_json::from_str::<Value>(body) {
        match parsed.get("error") {
            Some(Value::String(error)) => return Some(error.clone()),
            Some(error @ (Value::Object(_) | Value::Array(_))) => {
                if let Some(Value::String(nested)) = error.get("message") {
                    return Some(nested.clone());
                }
            }
            _ => {}
        }
        if let Some(Value::String(message)) = parsed.get("message") {
            return Some(message.clone());
        }
    }
    Some(body.to_owned())
}

fn parse_retry_after(header: &str, now: i64) -> Option<i64> {
    let trimmed = header.trim();
    let seconds = if trimmed.is_empty() {
        Some(0.0)
    } else {
        trimmed.parse::<f64>().ok()
    };
    if let Some(seconds) = seconds.filter(|seconds| seconds.is_finite() && *seconds >= 0.0) {
        return Some(js_round(seconds));
    }
    let at = parse_http_date(trimmed)?;
    let delta = (at - now) as f64 / 1000.0;
    Some(js_round(delta).max(0))
}

fn js_round(value: f64) -> i64 {
    (value + 0.5).floor() as i64
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
        })
}

fn parse_http_date(value: &str) -> Option<i64> {
    const MONTHS: [&str; 12] = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    let rest = value.split_once(',').map_or(value, |(_, rest)| rest);
    let parts = rest.split_whitespace().collect::<Vec<_>>();
    let [day, month, year, time, zone] = parts.as_slice() else {
        return None;
    };
    if !matches!(zone.to_ascii_uppercase().as_str(), "GMT" | "UTC" | "Z") {
        return None;
    }
    let day = day.parse::<i64>().ok()?;
    let month = MONTHS
        .iter()
        .position(|name| name.eq_ignore_ascii_case(month))?;
    let year = year.parse::<i64>().ok()?;
    let mut clock = time.split(':').map(str::parse::<i64>);
    let (Some(Ok(hours)), Some(Ok(minutes)), Some(Ok(seconds)), None) =
        (clock.next(), clock.next(), clock.next(), clock.next())
    else {
        return None;
    };
    if !(1..=31).contains(&day) || hours > 23 || minutes > 59 || seconds > 60 {
        return None;
    }
    let days = days_from_civil(year, i64::try_from(month).ok()? + 1, day);
    Some((((days * 24 + hours) * 60 + minutes) * 60 + seconds) * 1000)
}

const fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let shifted = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * shifted + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct Wrapped {
        message: &'static str,
        source: Option<Box<dyn StdError + 'static>>,
    }

    impl std::fmt::Display for Wrapped {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str(self.message)
        }
    }

    impl StdError for Wrapped {
        fn source(&self) -> Option<&(dyn StdError + 'static)> {
            self.source.as_deref()
        }
    }

    fn wrapped(message: &'static str, source: impl StdError + 'static) -> Wrapped {
        Wrapped {
            message,
            source: Some(Box::new(source)),
        }
    }

    fn plain(message: &'static str) -> Wrapped {
        Wrapped {
            message,
            source: None,
        }
    }

    fn detail<'a>(error: &'a AppError, key: &str) -> Option<&'a Value> {
        error.details.as_ref().and_then(|details| details.get(key))
    }

    #[test]
    fn each_vendor_failure_class_maps_onto_one_app_error_code() {
        for (status, code) in [
            (401, AppErrorCode::ProviderAuthFailed),
            (403, AppErrorCode::ProviderAuthFailed),
            (429, AppErrorCode::RateLimited),
            (408, AppErrorCode::ProviderUnreachable),
            (504, AppErrorCode::ProviderUnreachable),
            (503, AppErrorCode::ProviderUnreachable),
            (500, AppErrorCode::ProviderUnreachable),
            (418, AppErrorCode::Unknown),
            (404, AppErrorCode::Unknown),
        ] {
            let error = http_failure(HttpFailure::status(status));
            assert_eq!(error.code, code, "{status}");
            assert_eq!(detail(&error, "status"), Some(&json!(status)), "{status}");
        }
        let limited = http_failure(HttpFailure::status(429).with_header("Retry-After", "30"));
        assert_eq!(detail(&limited, "retryAfter"), Some(&json!(30)));
        let teapot = http_failure(
            HttpFailure::status(418)
                .with_body(json!({ "error": { "message": "нет" } }).to_string()),
        );
        assert_eq!(detail(&teapot, "vendorMessage"), Some(&json!("нет")));
        assert_eq!(raw_error_text(&teapot), "нет");
        assert!(teapot.message.contains("неизвестную"));
        let body = "Vendor error ".repeat(100);
        assert_eq!(
            raw_error_text(&http_failure(
                HttpFailure::status(500).with_body(body.clone())
            )),
            body
        );
        assert_eq!(
            AppErrorCode::ProviderSessionExpired.as_str(),
            "PROVIDER_SESSION_EXPIRED"
        );
    }

    #[test]
    fn an_account_mode_auth_failure_is_an_expired_session() {
        let error = http_failure(HttpFailure {
            auth_mode: Some(AuthMode::Account),
            ..HttpFailure::status(401)
        });
        assert!(is_session_expired(&error));
        assert_eq!(error.message, SESSION_EXPIRED_MESSAGE);
        let api = http_failure(HttpFailure {
            auth_mode: Some(AuthMode::Api),
            ..HttpFailure::status(403)
        });
        assert_eq!(api.code, AppErrorCode::ProviderAuthFailed);
    }

    #[test]
    fn the_vendor_message_is_read_from_the_shapes_vendors_use() {
        for (body, expected) in [
            (r#"{"error":"bad key"}"#, Some("bad key")),
            (r#"{"error":{"message":"nested"}}"#, Some("nested")),
            (r#"{"message":"top"}"#, Some("top")),
            (
                r#"{"detail":"Invalid API Key"}"#,
                Some(r#"{"detail":"Invalid API Key"}"#),
            ),
            (r#"{"error":{"code":1}}"#, Some(r#"{"error":{"code":1}}"#)),
            ("<html>gateway</html>", Some("<html>gateway</html>")),
            ("   ", None),
        ] {
            let error = http_failure(HttpFailure::status(400).with_body(body));
            assert_eq!(
                detail(&error, "vendorMessage").and_then(Value::as_str),
                expected,
                "{body}"
            );
        }
        let extra = http_failure(HttpFailure {
            details: ErrorDetails::from_iter([("path".to_owned(), json!("/models"))]),
            ..HttpFailure::status(404)
        });
        assert_eq!(detail(&extra, "path"), Some(&json!("/models")));
        assert!(is_not_found_response(&extra));
        assert!(!is_not_found_response(&http_failure(HttpFailure::status(
            500
        ))));
    }

    #[test]
    fn retry_after_takes_seconds_or_an_http_date() {
        let now = 1_445_412_480_000;
        for (header, expected) in [
            ("30", Some(30)),
            (" 2.5 ", Some(3)),
            ("", Some(0)),
            ("-1", None),
            ("Wed, 21 Oct 2015 07:28:00 GMT", Some(0)),
            ("Wed, 21 Oct 2015 07:28:30 GMT", Some(30)),
            ("Wed, 21 Oct 2015 07:27:00 GMT", Some(0)),
            ("soon", None),
        ] {
            assert_eq!(parse_retry_after(header, now), expected, "{header}");
        }
        assert_eq!(parse_http_date("Thu, 01 Jan 1970 00:00:00 GMT"), Some(0));
        assert_eq!(
            parse_http_date("Wed, 21 Oct 2015 07:28:00 GMT"),
            Some(1_445_412_480_000)
        );
        let at = http_failure_at(
            HttpFailure::status(429).with_header("retry-after", "Wed, 21 Oct 2015 07:28:10 GMT"),
            now,
        );
        assert_eq!(detail(&at, "retryAfter"), Some(&json!(10)));
    }

    #[test]
    fn network_failures_map_by_their_cause_chain() {
        for (kind, code) in [
            (io::ErrorKind::ConnectionRefused, "ECONNREFUSED"),
            (io::ErrorKind::ConnectionReset, "ECONNRESET"),
            (io::ErrorKind::TimedOut, "ETIMEDOUT"),
            (io::ErrorKind::HostUnreachable, "EHOSTUNREACH"),
            (io::ErrorKind::BrokenPipe, "EPIPE"),
        ] {
            let error = wrapped(
                "error sending request",
                wrapped("client error (Connect)", io::Error::new(kind, "os error")),
            );
            let mapped = network_failure(&error);
            assert_eq!(mapped.code, AppErrorCode::ProviderUnreachable, "{code}");
            assert_eq!(mapped.message, UNREACHABLE_MESSAGE);
            assert_eq!(detail(&mapped, "syscall"), Some(&json!(code)));
        }
        let dns = network_failure(&wrapped(
            "error sending request",
            plain("dns error: No such host is known. (os error 11001)"),
        ));
        assert_eq!(detail(&dns, "syscall"), Some(&json!("ENOTFOUND")));
        let slow = network_failure(&plain("operation timed out"));
        assert_eq!(slow.code, AppErrorCode::ProviderUnreachable);
        assert_eq!(slow.message, TIMED_OUT_MESSAGE);
        assert_eq!(slow.details, None);
        let boom = network_failure(&plain("boom"));
        assert_eq!(boom.code, AppErrorCode::Unknown);
        assert_eq!(boom.message, REQUEST_FAILED_MESSAGE);
    }

    #[test]
    fn raw_text_follows_the_cause_chain() {
        assert_eq!(
            raw_cause_text(&wrapped("fetch failed", plain("ECONNRESET"))),
            "fetch failed\nCaused by: ECONNRESET"
        );
        assert_eq!(raw_cause_text(&plain("plain warning")), "plain warning");
        assert_eq!(
            raw_error_text(&AppError::new(AppErrorCode::SecretMissing, "нет значения")),
            "нет значения"
        );
    }

    #[test]
    fn cancellation_and_timeouts_have_their_own_shapes() {
        assert!(is_cancellation(&cancelled()));
        assert_eq!(cancelled().message, CANCELLED_MESSAGE);
        assert_eq!(detail(&timed_out(60.0), "timeoutSeconds"), Some(&json!(60)));
        assert_eq!(
            detail(&timed_out(0.05), "timeoutSeconds"),
            Some(&json!(0.05))
        );
        assert_eq!(timed_out(1.0).code, AppErrorCode::ProviderUnreachable);
    }

    #[test]
    fn a_redirect_or_an_html_page_reads_as_signed_out() {
        assert!(is_signed_out_response(302, &Headers::new()));
        assert!(is_signed_out_response(
            200,
            &Headers::from([(
                "content-type".to_owned(),
                "Text/HTML; charset=utf-8".to_owned()
            )])
        ));
        assert!(!is_signed_out_response(
            200,
            &Headers::from([("content-type".to_owned(), "application/json".to_owned())])
        ));
    }

    #[test]
    fn html_classification_uses_login_destination_and_strong_challenge_evidence() {
        let headers = Headers::from([("content-type".to_owned(), "text/html".to_owned())]);
        for (body, url, reason) in [
            ("<html>gateway secret</html>", None, "unexpected-html"),
            (
                "<script>const label = 'security verification';</script>",
                None,
                "unexpected-html",
            ),
            (
                "<html><title>Checking your browser</title></html>",
                None,
                "browser-verification",
            ),
            (
                "<html>app</html>",
                Some("https://chat.qwen.ai/auth?token=secret"),
                "login-required",
            ),
        ] {
            let error =
                account_page_error(200, &headers, body, url, true, ErrorDetails::new()).unwrap();
            assert_eq!(detail(&error, "reason"), Some(&json!(reason)));
            assert!(!format!("{error:?}").contains("secret"));
        }
        assert!(
            account_page_error(200, &Headers::new(), "{}", None, false, ErrorDetails::new())
                .is_none()
        );
    }
}
