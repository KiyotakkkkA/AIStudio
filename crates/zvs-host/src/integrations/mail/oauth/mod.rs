pub mod google;
pub mod mailru;
pub mod yandex;

use zvs_contract::AppError;

use crate::drivers::mcp::oauth::Pkce;
use crate::integrations::oauth::AuthorizeRequest;

pub const PRESETS: [MailPreset; 3] = [google::PRESET, yandex::PRESET, mailru::PRESET];

pub use crate::integrations::oauth::{
    CodeExchange, OAuthClient, OAuthGrant as MailGrant, exchange, refresh, sign_in_required,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MailPreset {
    pub key: &'static str,
    pub label: &'static str,
    pub imap_host: &'static str,
    pub imap_port: u16,
    pub smtp_host: &'static str,
    pub smtp_port: u16,
    pub authorize_endpoint: &'static str,
    pub token_endpoint: &'static str,
    pub scope: &'static str,
    pub authorize_params: &'static [(&'static str, &'static str)],
}

pub fn preset(key: &str) -> Option<MailPreset> {
    PRESETS.into_iter().find(|preset| preset.key == key)
}

pub fn authorize_url(
    preset: &MailPreset,
    client: &OAuthClient<'_>,
    redirect_uri: &str,
    state: &str,
    pkce: &Pkce,
) -> Result<String, AppError> {
    crate::integrations::oauth::authorize_url(&AuthorizeRequest {
        endpoint: preset.authorize_endpoint,
        client,
        redirect_uri,
        scope: preset.scope,
        state,
        pkce,
        extra: preset.authorize_params,
    })
}

pub fn xoauth2(user: &str, access_token: &str) -> String {
    format!("user={user}\x01auth=Bearer {access_token}\x01\x01")
}

pub struct XOAuth2 {
    initial: Option<String>,
}

impl XOAuth2 {
    pub fn new(user: &str, access_token: &str) -> Self {
        Self {
            initial: Some(xoauth2(user, access_token)),
        }
    }
}

impl async_imap::Authenticator for XOAuth2 {
    type Response = String;

    fn process(&mut self, _challenge: &[u8]) -> Self::Response {
        self.initial.take().unwrap_or_default()
    }
}
