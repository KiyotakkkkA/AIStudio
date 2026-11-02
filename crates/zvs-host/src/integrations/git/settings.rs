use std::time::Duration;

use zvs_contract::{AppError, AppErrorCode, FieldDefault, FieldDescriptor, FieldKind};

use super::copy;
use super::forge::{Forge, repository};
use crate::integrations::IntegrationConfigView;
use crate::integrations::common::{
    ACCESS, READ_WRITE, access_field, field, list, number, option, optional_text, select, whole,
    with_help, with_placeholder,
};

pub const AUTH: &str = "auth";
pub const AUTH_TOKEN: &str = "token";
pub const AUTH_OAUTH: &str = "oauth";
pub const TOKEN: &str = "token";
pub const BASE_URL: &str = "baseUrl";
pub const OAUTH_CLIENT_ID: &str = "oauthClientId";
pub const OAUTH_CLIENT_SECRET: &str = "oauthClientSecret";
pub const OAUTH_REDIRECT_PORT: &str = "oauthRedirectPort";
pub const OAUTH_TOKEN: &str = "oauthToken";
pub const REPOSITORIES: &str = "repositories";
pub const POLL_SECONDS: &str = "pollSeconds";

pub const DEFAULT_POLL_SECONDS: u64 = 120;
pub const MIN_POLL_SECONDS: u64 = 30;
pub const MAX_POLL_SECONDS: u64 = 3600;
pub const MAX_REPOSITORIES: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMode {
    Token,
    OAuth,
}

pub struct GitSettings {
    pub base: String,
    pub auth: AuthMode,
    pub repositories: Vec<String>,
    pub poll: Duration,
    pub can_write: bool,
}

pub fn auth_mode(config: &IntegrationConfigView) -> AuthMode {
    if config.text(AUTH) == Some(AUTH_OAUTH) {
        AuthMode::OAuth
    } else {
        AuthMode::Token
    }
}

pub fn base(forge: Forge, config: &IntegrationConfigView) -> String {
    optional_text(config, BASE_URL)
        .unwrap_or_else(|| forge.default_base().to_owned())
        .trim_end_matches('/')
        .to_owned()
}

pub fn repositories(forge: Forge, config: &IntegrationConfigView) -> Result<Vec<String>, AppError> {
    let mut repositories = list(config.text(REPOSITORIES))
        .iter()
        .map(|entry| repository(forge, entry))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.with_detail("field", format!("config.{REPOSITORIES}")))?;
    repositories.truncate(MAX_REPOSITORIES);
    Ok(repositories)
}

pub fn load(forge: Forge, config: &IntegrationConfigView) -> Result<GitSettings, AppError> {
    Ok(GitSettings {
        base: base(forge, config),
        auth: auth_mode(config),
        repositories: repositories(forge, config)?,
        poll: Duration::from_secs(whole(
            config,
            POLL_SECONDS,
            DEFAULT_POLL_SECONDS,
            MIN_POLL_SECONDS,
            MAX_POLL_SECONDS,
        )),
        can_write: config.text(ACCESS) == Some(READ_WRITE),
    })
}

pub fn port(config: &IntegrationConfigView) -> Result<Option<u16>, AppError> {
    let Some(value) = config.number(OAUTH_REDIRECT_PORT) else {
        return Ok(None);
    };
    if value.fract() == 0.0 && (1.0..=65_535.0).contains(&value) {
        Ok(Some(value as u16))
    } else {
        Err(
            AppError::new(AppErrorCode::ValidationFailed, copy::REDIRECT_PORT_HELP)
                .with_detail("field", format!("config.{OAUTH_REDIRECT_PORT}")),
        )
    }
}

pub fn describe(forge: Forge) -> Vec<FieldDescriptor> {
    let (token_help, base_help, client_help, repositories_help, base_placeholder) = match forge {
        Forge::GitHub => (
            copy::GITHUB_TOKEN_HELP,
            copy::GITHUB_BASE_URL_HELP,
            copy::GITHUB_CLIENT_ID_HELP,
            copy::GITHUB_REPOSITORIES_HELP,
            "owner/repo, owner/other",
        ),
        Forge::GitLab => (
            copy::GITLAB_TOKEN_HELP,
            copy::GITLAB_BASE_URL_HELP,
            copy::GITLAB_CLIENT_ID_HELP,
            copy::GITLAB_REPOSITORIES_HELP,
            "group/project, group/subgroup/project",
        ),
    };
    let mut fields = vec![
        with_help(
            select(
                AUTH,
                copy::AUTH_LABEL,
                vec![
                    option(AUTH_TOKEN, copy::AUTH_TOKEN),
                    option(AUTH_OAUTH, copy::AUTH_OAUTH),
                ],
                AUTH_TOKEN,
            ),
            copy::AUTH_HELP,
        ),
        access_field(copy::ACCESS_HELP),
        with_help(
            field(TOKEN, copy::TOKEN_LABEL, FieldKind::Secret, false),
            token_help,
        ),
        with_help(
            FieldDescriptor {
                default: Some(FieldDefault::Text(forge.default_base().to_owned())),
                ..field(BASE_URL, copy::BASE_URL_LABEL, FieldKind::Url, false)
            },
            base_help,
        ),
        with_help(
            field(
                OAUTH_CLIENT_ID,
                copy::CLIENT_ID_LABEL,
                FieldKind::Text,
                false,
            ),
            client_help,
        ),
    ];
    if forge == Forge::GitLab {
        fields.push(with_help(
            field(
                OAUTH_CLIENT_SECRET,
                copy::CLIENT_SECRET_LABEL,
                FieldKind::Secret,
                false,
            ),
            copy::CLIENT_SECRET_HELP,
        ));
        fields.push(with_help(
            number(OAUTH_REDIRECT_PORT, copy::REDIRECT_PORT_LABEL, None),
            copy::REDIRECT_PORT_HELP,
        ));
    }
    fields.extend([
        with_help(
            field(
                OAUTH_TOKEN,
                copy::OAUTH_TOKEN_LABEL,
                FieldKind::Secret,
                false,
            ),
            copy::OAUTH_TOKEN_HELP,
        ),
        with_placeholder(
            with_help(
                field(
                    REPOSITORIES,
                    copy::REPOSITORIES_LABEL,
                    FieldKind::Text,
                    false,
                ),
                repositories_help,
            ),
            base_placeholder,
        ),
        with_help(
            number(POLL_SECONDS, copy::POLL_LABEL, Some(DEFAULT_POLL_SECONDS)),
            copy::POLL_HELP,
        ),
    ]);
    fields
}
