use std::time::Duration;

use zvs_contract::{AppError, AppErrorCode, FieldDefault, FieldDescriptor, FieldKind, FieldOption};

use super::copy;
use super::oauth::{MailPreset, PRESETS, preset};
use crate::integrations::{FIELD_REQUIRED, IntegrationConfigView};
use crate::services::crypto::Plaintext;

pub const PROTOCOL: &str = "protocol";
pub const ACCESS: &str = "access";
pub const READ_ONLY: &str = "read";
pub const READ_WRITE: &str = "readWrite";
pub const ADDRESS: &str = "address";
pub const DISPLAY_NAME: &str = "displayName";
pub const IMAP_HOST: &str = "imapHost";
pub const IMAP_PORT: &str = "imapPort";
pub const IMAP_SECURITY: &str = "imapSecurity";
pub const IMAP_USER: &str = "imapUser";
pub const IMAP_PASSWORD: &str = "imapPassword";
pub const SMTP_HOST: &str = "smtpHost";
pub const SMTP_PORT: &str = "smtpPort";
pub const SMTP_SECURITY: &str = "smtpSecurity";
pub const SMTP_REUSE_LOGIN: &str = "smtpReuseLogin";
pub const SMTP_USER: &str = "smtpUser";
pub const SMTP_PASSWORD: &str = "smtpPassword";
pub const OAUTH_CLIENT_ID: &str = "oauthClientId";
pub const OAUTH_CLIENT_SECRET: &str = "oauthClientSecret";
pub const OAUTH_REDIRECT_PORT: &str = "oauthRedirectPort";
pub const OAUTH_TOKEN: &str = "oauthToken";
pub const FOLDERS: &str = "folders";
pub const POLL_SECONDS: &str = "pollSeconds";
pub const IGNORE_OLDER_DAYS: &str = "ignoreOlderDays";
pub const MARK_READ: &str = "markRead";

pub const IMAP_PROTOCOL: &str = "imap";
pub const DEFAULT_FOLDER: &str = "INBOX";
pub const DEFAULT_POLL_SECONDS: u64 = 60;
pub const MIN_POLL_SECONDS: u64 = 15;
pub const MAX_POLL_SECONDS: u64 = 86_400;
pub const DEFAULT_IGNORE_OLDER_DAYS: u64 = 7;
pub const MAX_IGNORE_OLDER_DAYS: u64 = 3650;
pub const MAX_FOLDERS: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Security {
    Tls,
    StartTls,
}

impl Security {
    fn parse(value: Option<&str>) -> Self {
        match value {
            Some("starttls") => Self::StartTls,
            _ => Self::Tls,
        }
    }
}

pub enum Auth {
    Password(Plaintext),
    Bearer(Plaintext),
}

pub struct Endpoint {
    pub host: String,
    pub port: u16,
    pub security: Security,
    pub user: String,
    pub auth: Auth,
}

pub struct MailSettings {
    pub address: String,
    pub display_name: Option<String>,
    pub imap: Endpoint,
    pub smtp: Endpoint,
    pub folders: Vec<String>,
    pub poll: Duration,
    pub ignore_older_days: u64,
    pub mark_read: bool,
    pub can_send: bool,
}

pub fn can_send(config: &zvs_contract::IntegrationConfig) -> bool {
    matches!(
        config.get(ACCESS),
        Some(zvs_contract::IntegrationValue::Text(value)) if value == READ_WRITE
    )
}

impl MailSettings {
    pub fn sender(&self) -> String {
        match &self.display_name {
            Some(name) => format!("{name} <{}>", self.address),
            None => self.address.clone(),
        }
    }
}

pub fn required_text(config: &IntegrationConfigView, key: &str) -> Result<String, AppError> {
    optional_text(config, key).ok_or_else(|| {
        AppError::new(AppErrorCode::ValidationFailed, FIELD_REQUIRED)
            .with_detail("field", format!("config.{key}"))
            .with_detail("rule", "required")
    })
}

pub fn optional_text(config: &IntegrationConfigView, key: &str) -> Option<String> {
    config
        .text(key)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

pub fn port(config: &IntegrationConfigView, key: &str) -> Result<Option<u16>, AppError> {
    let Some(value) = config.number(key) else {
        return Ok(None);
    };
    if value.fract() == 0.0 && (1.0..=65_535.0).contains(&value) {
        Ok(Some(value as u16))
    } else {
        Err(
            AppError::new(AppErrorCode::ValidationFailed, copy::BAD_PORT)
                .with_detail("field", format!("config.{key}")),
        )
    }
}

fn whole(config: &IntegrationConfigView, key: &str, fallback: u64, min: u64, max: u64) -> u64 {
    config
        .number(key)
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map_or(fallback, |value| value.round() as u64)
        .clamp(min, max)
}

pub fn folders(raw: Option<&str>) -> Vec<String> {
    let mut folders: Vec<String> = Vec::new();
    for name in raw.unwrap_or(DEFAULT_FOLDER).split([',', '\n']) {
        let name = name.trim();
        if !name.is_empty() && !folders.iter().any(|known| known == name) {
            folders.push(name.to_owned());
        }
    }
    if folders.is_empty() {
        folders.push(DEFAULT_FOLDER.to_owned());
    }
    folders.truncate(MAX_FOLDERS);
    folders
}

pub fn oauth_preset(config: &IntegrationConfigView) -> Result<Option<MailPreset>, AppError> {
    match config.text(PROTOCOL) {
        None | Some(IMAP_PROTOCOL) => Ok(None),
        Some(key) => preset(key).map(Some).ok_or_else(|| {
            AppError::new(AppErrorCode::NotImplemented, copy::PROTOCOL_UNAVAILABLE)
                .with_detail("field", format!("config.{PROTOCOL}"))
        }),
    }
}

fn manual(config: &IntegrationConfigView) -> Result<(Endpoint, Endpoint), AppError> {
    let imap_security = Security::parse(config.text(IMAP_SECURITY));
    let smtp_security = Security::parse(config.text(SMTP_SECURITY));
    let imap = Endpoint {
        host: required_text(config, IMAP_HOST)?,
        port: port(config, IMAP_PORT)?.unwrap_or(match imap_security {
            Security::Tls => 993,
            Security::StartTls => 143,
        }),
        security: imap_security,
        user: required_text(config, IMAP_USER)?,
        auth: Auth::Password(config.secret(IMAP_PASSWORD)?),
    };
    let (user, password) = if config.flag(SMTP_REUSE_LOGIN).unwrap_or(true) {
        (imap.user.clone(), config.secret(IMAP_PASSWORD)?)
    } else {
        (
            required_text(config, SMTP_USER)?,
            config.secret(SMTP_PASSWORD)?,
        )
    };
    let smtp = Endpoint {
        host: required_text(config, SMTP_HOST)?,
        port: port(config, SMTP_PORT)?.unwrap_or(match smtp_security {
            Security::Tls => 465,
            Security::StartTls => 587,
        }),
        security: smtp_security,
        user,
        auth: Auth::Password(password),
    };
    Ok((imap, smtp))
}

fn preset_endpoints(preset: &MailPreset, address: &str, token: &str) -> (Endpoint, Endpoint) {
    let endpoint = |host: &str, port: u16| Endpoint {
        host: host.to_owned(),
        port,
        security: Security::Tls,
        user: address.to_owned(),
        auth: Auth::Bearer(Plaintext::new(token)),
    };
    (
        endpoint(preset.imap_host, preset.imap_port),
        endpoint(preset.smtp_host, preset.smtp_port),
    )
}

pub fn load(
    config: &IntegrationConfigView,
    bearer: Option<&str>,
) -> Result<MailSettings, AppError> {
    let address = required_text(config, ADDRESS)?;
    let (imap, smtp) = match (oauth_preset(config)?, bearer) {
        (Some(preset), Some(token)) => preset_endpoints(&preset, &address, token),
        (Some(_), None) => return Err(super::oauth::sign_in_required()),
        (None, _) => manual(config)?,
    };
    Ok(MailSettings {
        address,
        display_name: optional_text(config, DISPLAY_NAME),
        imap,
        smtp,
        folders: folders(config.text(FOLDERS)),
        poll: Duration::from_secs(whole(
            config,
            POLL_SECONDS,
            DEFAULT_POLL_SECONDS,
            MIN_POLL_SECONDS,
            MAX_POLL_SECONDS,
        )),
        ignore_older_days: whole(
            config,
            IGNORE_OLDER_DAYS,
            DEFAULT_IGNORE_OLDER_DAYS,
            0,
            MAX_IGNORE_OLDER_DAYS,
        ),
        mark_read: config.flag(MARK_READ).unwrap_or(false),
        can_send: config.text(ACCESS) == Some(READ_WRITE),
    })
}

fn field(key: &str, label: &str, kind: FieldKind, required: bool) -> FieldDescriptor {
    FieldDescriptor {
        key: key.to_owned(),
        label: label.to_owned(),
        kind,
        required,
        placeholder: None,
        help: None,
        options: None,
        default: None,
    }
}

fn option(value: &str, label: &str) -> FieldOption {
    FieldOption {
        value: value.to_owned(),
        label: label.to_owned(),
    }
}

fn security(key: &str, label: &str) -> FieldDescriptor {
    FieldDescriptor {
        options: Some(vec![
            option("tls", "SSL/TLS"),
            option("starttls", "STARTTLS"),
        ]),
        default: Some(FieldDefault::Text("tls".to_owned())),
        help: Some(copy::MANUAL_ONLY_HELP.to_owned()),
        ..field(key, label, FieldKind::Select, false)
    }
}

fn number(key: &str, label: &str, default: Option<u64>) -> FieldDescriptor {
    FieldDescriptor {
        default: default.map(|value| FieldDefault::Number(value.into())),
        ..field(key, label, FieldKind::Number, false)
    }
}

fn with_help(field: FieldDescriptor, help: &str) -> FieldDescriptor {
    FieldDescriptor {
        help: Some(help.to_owned()),
        ..field
    }
}

fn with_placeholder(field: FieldDescriptor, placeholder: &str) -> FieldDescriptor {
    FieldDescriptor {
        placeholder: Some(placeholder.to_owned()),
        ..field
    }
}

fn manual_only(field: FieldDescriptor) -> FieldDescriptor {
    with_help(field, copy::MANUAL_ONLY_HELP)
}

pub fn describe() -> Vec<FieldDescriptor> {
    let mut protocols = vec![option(IMAP_PROTOCOL, "IMAP / SMTP")];
    protocols.extend(
        PRESETS
            .iter()
            .map(|preset| option(preset.key, preset.label)),
    );
    vec![
        with_help(
            FieldDescriptor {
                options: Some(protocols),
                default: Some(FieldDefault::Text(IMAP_PROTOCOL.to_owned())),
                ..field(PROTOCOL, "Протокол", FieldKind::Select, false)
            },
            copy::PROTOCOL_HELP,
        ),
        with_help(
            FieldDescriptor {
                options: Some(vec![
                    option(READ_ONLY, "Только чтение"),
                    option(READ_WRITE, "Чтение и отправка"),
                ]),
                default: Some(FieldDefault::Text(READ_ONLY.to_owned())),
                ..field(ACCESS, "Доступ", FieldKind::Select, false)
            },
            copy::ACCESS_HELP,
        ),
        with_placeholder(
            field(ADDRESS, "Адрес", FieldKind::Text, true),
            "support@example.com",
        ),
        field(DISPLAY_NAME, "Имя отправителя", FieldKind::Text, false),
        with_placeholder(
            manual_only(field(IMAP_HOST, "Сервер IMAP", FieldKind::Text, false)),
            "imap.example.com",
        ),
        manual_only(number(IMAP_PORT, "Порт IMAP", Some(993))),
        security(IMAP_SECURITY, "Шифрование IMAP"),
        manual_only(field(IMAP_USER, "Логин IMAP", FieldKind::Text, false)),
        manual_only(field(
            IMAP_PASSWORD,
            "Пароль IMAP",
            FieldKind::Secret,
            false,
        )),
        with_placeholder(
            manual_only(field(SMTP_HOST, "Сервер SMTP", FieldKind::Text, false)),
            "smtp.example.com",
        ),
        manual_only(number(SMTP_PORT, "Порт SMTP", Some(465))),
        security(SMTP_SECURITY, "Шифрование SMTP"),
        manual_only(FieldDescriptor {
            default: Some(FieldDefault::Boolean(true)),
            ..field(
                SMTP_REUSE_LOGIN,
                "Использовать учётные данные IMAP",
                FieldKind::Boolean,
                false,
            )
        }),
        manual_only(field(SMTP_USER, "Логин SMTP", FieldKind::Text, false)),
        manual_only(field(
            SMTP_PASSWORD,
            "Пароль SMTP",
            FieldKind::Secret,
            false,
        )),
        with_help(
            field(
                OAUTH_CLIENT_ID,
                "Идентификатор приложения OAuth",
                FieldKind::Text,
                false,
            ),
            copy::CLIENT_ID_HELP,
        ),
        with_help(
            field(
                OAUTH_CLIENT_SECRET,
                "Секрет приложения OAuth",
                FieldKind::Secret,
                false,
            ),
            copy::CLIENT_ID_HELP,
        ),
        with_help(
            number(OAUTH_REDIRECT_PORT, "Порт адреса возврата", None),
            copy::REDIRECT_PORT_HELP,
        ),
        with_help(
            field(OAUTH_TOKEN, "Вход OAuth", FieldKind::Secret, false),
            copy::OAUTH_TOKEN_HELP,
        ),
        with_help(
            FieldDescriptor {
                default: Some(FieldDefault::Text(DEFAULT_FOLDER.to_owned())),
                ..field(FOLDERS, "Отслеживаемые папки", FieldKind::Text, false)
            },
            copy::FOLDERS_HELP,
        ),
        with_help(
            number(
                POLL_SECONDS,
                "Интервал опроса, с",
                Some(DEFAULT_POLL_SECONDS),
            ),
            copy::POLL_HELP,
        ),
        with_help(
            number(
                IGNORE_OLDER_DAYS,
                "Пропускать письма старше, дней",
                Some(DEFAULT_IGNORE_OLDER_DAYS),
            ),
            copy::IGNORE_OLDER_HELP,
        ),
        with_help(
            FieldDescriptor {
                default: Some(FieldDefault::Boolean(false)),
                ..field(
                    MARK_READ,
                    "Отмечать прочитанным после обработки",
                    FieldKind::Boolean,
                    false,
                )
            },
            copy::MARK_READ_HELP,
        ),
    ]
}
