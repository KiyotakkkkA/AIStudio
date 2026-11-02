mod copy;
mod imap;
mod message;
pub mod oauth;
mod settings;
mod smtp;
mod sync;
mod tls;

use std::sync::Arc;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use zvs_contract::{
    AppError, AppErrorCode, FieldDescriptor, IntegrationAccountDto, IntegrationCheckDto,
    IntegrationKind, IntegrationToolDto, IntegrationTriggerDto, MAIL_APPROVAL_KIND, MailDraftDto,
    MailMessageDto, MailRefDto,
};

use super::{
    APPROVAL_UNAVAILABLE, IntegrationApproval, IntegrationConfigView, IntegrationHandled,
    IntegrationProvider, IntegrationSession, IntegrationToolCall, IntegrationToolSpec,
    SignInRequest, SignedIn,
};
use crate::drivers::mcp::oauth::{Loopback, Pkce, SIGN_IN_TIMEOUT, random_state};
use crate::kernel::input_schema;
use crate::platform::clock::{SharedClock, SystemClock};
use crate::platform::ids::new_id;
use crate::services::crypto::Plaintext;
use crate::workers::WorkerFault;
use imap::{ImapConnection, quote};
use message::{body, clip, reply_subject, summary, thread_references};
use oauth::{CodeExchange, MailGrant, MailPreset, OAuthClient, sign_in_required};
use settings::MailSettings;
use smtp::mailbox;
pub use sync::RECEIVED;
pub use tls::MailTrust;

pub use copy::{SEND_DENIED, UNKNOWN_TOOL};
pub use settings::{
    ACCESS, ADDRESS, FOLDERS, IGNORE_OLDER_DAYS, IMAP_HOST, IMAP_PASSWORD, IMAP_PORT,
    IMAP_SECURITY, IMAP_USER, MARK_READ, OAUTH_CLIENT_ID, OAUTH_CLIENT_SECRET, OAUTH_REDIRECT_PORT,
    OAUTH_TOKEN, POLL_SECONDS, PROTOCOL, READ_ONLY, READ_WRITE, SMTP_HOST, SMTP_PASSWORD,
    SMTP_PORT, SMTP_REUSE_LOGIN, SMTP_SECURITY, SMTP_USER,
};

pub const SEND: &str = "mail.send";
pub const SEARCH: &str = "mail.search";
pub const READ: &str = "mail.read";
pub const SEARCH_LIMIT_DEFAULT: u64 = 20;
pub const SEARCH_LIMIT_MAX: u64 = 50;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SendInput {
    #[schemars(description = "Адреса получателей")]
    pub to: Vec<String>,
    #[serde(default)]
    #[schemars(description = "Адреса получателей копии")]
    pub cc: Vec<String>,
    #[serde(default)]
    #[schemars(description = "Тема; для ответа можно не указывать — подставится «Re: …»")]
    pub subject: Option<String>,
    #[schemars(description = "Текст письма")]
    pub text: String,
    #[serde(default)]
    #[schemars(description = "ref письма, на которое отправляется ответ")]
    pub reply_to: Option<MailRefDto>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchInput {
    #[serde(default)]
    #[schemars(description = "Папка; по умолчанию INBOX")]
    pub folder: Option<String>,
    #[serde(default)]
    #[schemars(description = "Письма начиная с этой даты, ГГГГ-ММ-ДД")]
    pub since: Option<String>,
    #[serde(default)]
    #[schemars(description = "Письма до этой даты, ГГГГ-ММ-ДД, не включая её")]
    pub before: Option<String>,
    #[serde(default)]
    #[schemars(description = "Адрес или имя отправителя")]
    pub from: Option<String>,
    #[serde(default)]
    #[schemars(description = "Текст в заголовках или теле письма")]
    pub text: Option<String>,
    #[serde(default)]
    #[schemars(description = "Сколько писем вернуть, от 1 до 50; по умолчанию 20")]
    pub limit: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadInput {
    #[serde(rename = "ref")]
    #[schemars(
        rename = "ref",
        description = "ref письма из mail.search или mail.received"
    )]
    pub reference: MailRefDto,
}

fn parse_input<T: for<'de> Deserialize<'de>>(input: &Map<String, Value>) -> Result<T, AppError> {
    serde_json::from_value(Value::Object(input.clone()))
        .map_err(|error| AppError::new(AppErrorCode::ValidationFailed, error.to_string()))
}

fn check(label: &str, outcome: Result<String, AppError>) -> IntegrationCheckDto {
    match outcome {
        Ok(detail) => IntegrationCheckDto {
            label: label.to_owned(),
            ok: true,
            detail: Some(detail),
        },
        Err(error) => IntegrationCheckDto {
            label: label.to_owned(),
            ok: false,
            detail: Some(error.message),
        },
    }
}

fn tool(name: &str, label: &str, description: &str, side_effect: bool) -> IntegrationToolDto {
    IntegrationToolDto {
        name: name.to_owned(),
        label: label.to_owned(),
        description: description.to_owned(),
        side_effect,
    }
}

pub fn search_date(field: &str, value: &str) -> Result<String, AppError> {
    let bad = || {
        AppError::new(AppErrorCode::ValidationFailed, copy::BAD_DATE).with_detail("field", field)
    };
    let mut parts = value.trim().split('-');
    let (Some(year), Some(month), Some(day), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(bad());
    };
    let year: u32 = year.parse().map_err(|_| bad())?;
    let month: usize = month.parse().map_err(|_| bad())?;
    let day: u32 = day.parse().map_err(|_| bad())?;
    let names = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let name = names.get(month.wrapping_sub(1)).ok_or_else(bad)?;
    if !(1..=31).contains(&day) || !(1900..=9999).contains(&year) {
        return Err(bad());
    }
    Ok(format!("{day}-{name}-{year}"))
}

fn criterion(name: &str, value: &str) -> (String, bool) {
    (format!("{name} {}", quote(value)), !value.is_ascii())
}

pub fn search_query(input: &SearchInput) -> Result<String, AppError> {
    let mut parts = vec!["ALL".to_owned()];
    let mut unicode = false;
    if let Some(since) = &input.since {
        parts.push(format!("SINCE {}", search_date("since", since)?));
    }
    if let Some(before) = &input.before {
        parts.push(format!("BEFORE {}", search_date("before", before)?));
    }
    for (name, value) in [("FROM", &input.from), ("TEXT", &input.text)] {
        if let Some(value) = value
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let (part, wide) = criterion(name, value);
            parts.push(part);
            unicode |= wide;
        }
    }
    let query = parts.join(" ");
    Ok(if unicode {
        format!("CHARSET UTF-8 {query}")
    } else {
        query
    })
}

fn recipients(values: &[String]) -> Result<Vec<String>, AppError> {
    values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(|value| mailbox(value).map(|_| value.to_owned()))
        .collect()
}

fn message_id(address: &str) -> String {
    let domain = address
        .rsplit_once('@')
        .map_or("localhost", |(_, domain)| domain.trim_end_matches('>'));
    format!("<{}@{domain}>", new_id().simple())
}

fn refuse_unless_sending(settings: &MailSettings) -> Result<(), AppError> {
    if settings.can_send {
        Ok(())
    } else {
        Err(
            AppError::new(AppErrorCode::PermissionDenied, copy::SEND_DISABLED)
                .with_detail("field", format!("config.{}", settings::ACCESS)),
        )
    }
}

pub struct MailProvider {
    trust: MailTrust,
    clock: SharedClock,
    http: reqwest::Client,
    token_endpoint: Option<String>,
    refreshing: tokio::sync::Mutex<()>,
}

impl MailProvider {
    pub fn new(http: reqwest::Client) -> Self {
        Self {
            trust: MailTrust::System,
            clock: Arc::new(SystemClock),
            http,
            token_endpoint: None,
            refreshing: tokio::sync::Mutex::new(()),
        }
    }

    #[must_use]
    pub fn with_trust(self, trust: MailTrust) -> Self {
        Self { trust, ..self }
    }

    #[must_use]
    pub fn with_clock(self, clock: SharedClock) -> Self {
        Self { clock, ..self }
    }

    #[must_use]
    pub fn with_token_endpoint(self, endpoint: impl Into<String>) -> Self {
        Self {
            token_endpoint: Some(endpoint.into()),
            ..self
        }
    }

    fn token_endpoint<'a>(&'a self, preset: &'a MailPreset) -> &'a str {
        self.token_endpoint
            .as_deref()
            .unwrap_or(preset.token_endpoint)
    }

    fn client_secret(config: &IntegrationConfigView) -> Result<Option<Plaintext>, AppError> {
        config
            .secret_id(OAUTH_CLIENT_SECRET)
            .map(|_| config.secret(OAUTH_CLIENT_SECRET))
            .transpose()
    }

    async fn bearer(
        &self,
        config: &IntegrationConfigView,
        preset: &MailPreset,
    ) -> Result<String, AppError> {
        if config.secret_id(OAUTH_TOKEN).is_none() {
            return Err(sign_in_required());
        }
        let current = MailGrant::parse(config.secret(OAUTH_TOKEN)?.expose())?;
        if current.provider != preset.key {
            return Err(sign_in_required());
        }
        let now = self.clock.now();
        if !current.expires_soon(now) {
            return Ok(current.access_token);
        }
        let _guard = self.refreshing.lock().await;
        let current = MailGrant::parse(config.secret(OAUTH_TOKEN)?.expose())?;
        if !current.expires_soon(now) {
            return Ok(current.access_token);
        }
        let secret = Self::client_secret(config)?;
        let client_id = settings::optional_text(config, OAUTH_CLIENT_ID)
            .unwrap_or_else(|| current.client_id.clone());
        let renewed = oauth::refresh(
            &self.http,
            self.token_endpoint(preset),
            &OAuthClient {
                id: &client_id,
                secret: secret.as_ref().map(Plaintext::expose),
            },
            &current,
            now,
        )
        .await?;
        config.update_secret(OAUTH_TOKEN, Plaintext::new(renewed.encode()))?;
        Ok(renewed.access_token)
    }

    async fn settings(&self, config: &IntegrationConfigView) -> Result<MailSettings, AppError> {
        let bearer = match settings::oauth_preset(config)? {
            Some(preset) => Some(self.bearer(config, &preset).await?),
            None => None,
        };
        settings::load(config, bearer.as_deref())
    }

    async fn connect(&self, settings: &MailSettings) -> Result<ImapConnection, AppError> {
        ImapConnection::connect(&settings.imap, &self.trust).await
    }

    async fn original(
        &self,
        settings: &MailSettings,
        reference: &MailRefDto,
    ) -> Result<MailMessageDto, AppError> {
        let mut imap = self.connect(settings).await?;
        let outcome = self.locate(&mut imap, reference).await;
        imap.logout().await;
        outcome.map(|fetched| summary(&fetched))
    }

    async fn locate(
        &self,
        imap: &mut ImapConnection,
        reference: &MailRefDto,
    ) -> Result<message::Fetched, AppError> {
        let gone = || {
            AppError::new(AppErrorCode::NotFound, copy::MESSAGE_GONE)
                .with_detail("uid", reference.uid.to_string())
        };
        let mailbox = imap.examine(&reference.folder).await?;
        if mailbox.uid_validity != reference.uid_validity {
            return Err(gone());
        }
        imap.fetch(&reference.folder, mailbox.uid_validity, &[reference.uid])
            .await?
            .into_iter()
            .find(|fetched| fetched.reference.uid == reference.uid)
            .ok_or_else(gone)
    }

    async fn draft(
        &self,
        settings: &MailSettings,
        input: &SendInput,
    ) -> Result<MailDraftDto, AppError> {
        let to = recipients(&input.to)?;
        if to.is_empty() {
            return Err(
                AppError::new(AppErrorCode::ValidationFailed, copy::NO_RECIPIENT)
                    .with_detail("field", "to"),
            );
        }
        let cc = recipients(&input.cc)?;
        let subject = input
            .subject
            .as_deref()
            .map(str::trim)
            .filter(|subject| !subject.is_empty())
            .map(str::to_owned);
        let (subject, in_reply_to, references) = match &input.reply_to {
            Some(reference) => {
                let original = self.original(settings, reference).await?;
                (
                    subject.unwrap_or_else(|| reply_subject(&original.subject.text)),
                    original.message_id.clone(),
                    thread_references(&original),
                )
            }
            None => (subject.unwrap_or_default(), None, Vec::new()),
        };
        let draft = MailDraftDto {
            from: settings.sender(),
            to,
            cc,
            subject: clip(&subject, zvs_contract::mail::MAIL_SUBJECT_MAX).0,
            text: input.text.clone(),
            in_reply_to,
            references,
        };
        zvs_contract::Validate::validate(&draft)?;
        mailbox(&draft.from)?;
        Ok(draft)
    }

    async fn send(
        &self,
        settings: &MailSettings,
        call: &IntegrationToolCall,
    ) -> Result<Value, AppError> {
        refuse_unless_sending(settings)?;
        let draft: MailDraftDto = call
            .approved
            .as_ref()
            .and_then(|approved| approved.get("draft"))
            .and_then(|draft| serde_json::from_value(draft.clone()).ok())
            .ok_or_else(|| AppError::new(AppErrorCode::ApprovalDenied, APPROVAL_UNAVAILABLE))?;
        let id = message_id(&settings.address);
        let letter = smtp::compose(&draft, &id)?;
        smtp::send(&settings.smtp, &self.trust, letter).await?;
        Ok(json!({
            "sent": true,
            "messageId": id,
            "to": draft.to,
            "cc": draft.cc,
            "subject": draft.subject,
            "inReplyTo": draft.in_reply_to,
            "text": copy::sent(&draft.to),
        }))
    }

    async fn search(
        &self,
        settings: &MailSettings,
        input: &SearchInput,
    ) -> Result<Value, AppError> {
        let folder = input
            .folder
            .as_deref()
            .map(str::trim)
            .filter(|folder| !folder.is_empty())
            .unwrap_or(settings::DEFAULT_FOLDER)
            .to_owned();
        let limit = input
            .limit
            .unwrap_or(SEARCH_LIMIT_DEFAULT)
            .clamp(1, SEARCH_LIMIT_MAX);
        let query = search_query(input)?;
        let mut imap = self.connect(settings).await?;
        let outcome = async {
            let mailbox = imap.examine(&folder).await?;
            let uids = imap.search(&query).await?;
            let skip = uids
                .len()
                .saturating_sub(usize::try_from(limit).unwrap_or(usize::MAX));
            let newest = &uids[skip..];
            let mut messages: Vec<MailMessageDto> = imap
                .fetch(&folder, mailbox.uid_validity, newest)
                .await?
                .iter()
                .map(summary)
                .collect();
            messages.reverse();
            Ok::<_, AppError>(json!({
                "untrusted": true,
                "folder": folder,
                "total": uids.len(),
                "messages": messages,
            }))
        }
        .await;
        imap.logout().await;
        outcome
    }

    async fn read(&self, settings: &MailSettings, input: &ReadInput) -> Result<Value, AppError> {
        let mut imap = self.connect(settings).await?;
        let outcome = self.locate(&mut imap, &input.reference).await;
        imap.logout().await;
        Ok(json!(body(&outcome?)))
    }
}

#[async_trait]
impl IntegrationProvider for MailProvider {
    fn kind(&self) -> IntegrationKind {
        IntegrationKind::Mail
    }

    fn label(&self) -> String {
        copy::LABEL.to_owned()
    }

    fn describe(&self) -> Vec<FieldDescriptor> {
        settings::describe()
    }

    fn triggers(&self) -> Vec<IntegrationTriggerDto> {
        vec![IntegrationTriggerDto {
            name: RECEIVED.to_owned(),
            label: copy::RECEIVED_LABEL.to_owned(),
            description: copy::RECEIVED_DESCRIPTION.to_owned(),
        }]
    }

    fn tools(&self) -> Vec<IntegrationToolSpec> {
        vec![
            IntegrationToolSpec::new(
                tool(SEND, copy::SEND_LABEL, copy::SEND_DESCRIPTION, true),
                input_schema::<SendInput>(),
            )
            .approved_per_call(),
            IntegrationToolSpec::new(
                tool(SEARCH, copy::SEARCH_LABEL, copy::SEARCH_DESCRIPTION, false),
                input_schema::<SearchInput>(),
            ),
            IntegrationToolSpec::new(
                tool(READ, copy::READ_LABEL, copy::READ_DESCRIPTION, false),
                input_schema::<ReadInput>(),
            ),
        ]
    }

    fn account_tools(&self, config: &zvs_contract::IntegrationConfig) -> Vec<IntegrationToolSpec> {
        let sending = settings::can_send(config);
        self.tools()
            .into_iter()
            .filter(|spec| sending || spec.tool.name != SEND)
            .collect()
    }

    async fn test(
        &self,
        config: &IntegrationConfigView,
    ) -> Result<Vec<IntegrationCheckDto>, AppError> {
        let settings = self.settings(config).await?;
        let imap = async {
            let mut imap = self.connect(&settings).await?;
            let count = imap.folder_count().await;
            imap.logout().await;
            Ok(copy::folders_found(count?))
        };
        let smtp = async {
            smtp::probe(&settings.smtp, &self.trust)
                .await
                .map(|()| copy::SMTP_READY.to_owned())
        };
        let (imap, smtp) = tokio::join!(imap, smtp);
        Ok(vec![
            check(copy::IMAP_CHECK, imap),
            check(copy::SMTP_CHECK, smtp),
        ])
    }

    async fn start(&self, session: IntegrationSession) -> Result<(), WorkerFault> {
        let settings = self.settings(&session.config).await.map_err(|error| {
            if matches!(
                error.code,
                AppErrorCode::ValidationFailed
                    | AppErrorCode::NotImplemented
                    | AppErrorCode::ProviderSessionExpired
                    | AppErrorCode::SecretMissing
            ) {
                WorkerFault::permanent(error)
            } else {
                WorkerFault::retry(error)
            }
        })?;
        sync::Sync {
            session: &session,
            settings: &settings,
            clock: &self.clock,
        }
        .run(&self.trust)
        .await
    }

    async fn call_tool(&self, call: IntegrationToolCall) -> Result<Value, AppError> {
        let settings = self.settings(&call.config).await?;
        match call.tool.as_str() {
            SEND => self.send(&settings, &call).await,
            SEARCH => self.search(&settings, &parse_input(&call.input)?).await,
            READ => self.read(&settings, &parse_input(&call.input)?).await,
            _ => Err(AppError::new(AppErrorCode::NotFound, UNKNOWN_TOOL)
                .with_detail("tool", call.tool.as_str())),
        }
    }

    async fn approval(
        &self,
        call: &IntegrationToolCall,
    ) -> Result<Option<IntegrationApproval>, AppError> {
        if call.tool != SEND {
            return Ok(None);
        }
        let settings = self.settings(&call.config).await?;
        refuse_unless_sending(&settings)?;
        let input: SendInput = parse_input(&call.input)?;
        let draft = self.draft(&settings, &input).await?;
        let mut fields = Map::new();
        fields.insert("account".to_owned(), json!(call.account.name));
        fields.insert("draft".to_owned(), json!(draft));
        Ok(Some(IntegrationApproval {
            kind: MAIL_APPROVAL_KIND.to_owned(),
            action: clip(
                &copy::sending(&draft.to),
                zvs_contract::mail::MAIL_ACTION_MAX,
            )
            .0,
            denied: SEND_DENIED.to_owned(),
            fields,
        }))
    }

    async fn sign_in(&self, request: SignInRequest<'_>) -> Result<SignedIn, AppError> {
        let config = request.config;
        let preset = settings::oauth_preset(config)?.ok_or_else(|| {
            AppError::new(AppErrorCode::ValidationFailed, copy::OAUTH_NOT_USED)
                .with_detail("field", format!("config.{PROTOCOL}"))
        })?;
        settings::required_text(config, ADDRESS)?;
        let client_id = settings::required_text(config, OAUTH_CLIENT_ID)?;
        let secret = Self::client_secret(config)?;
        let wanted = settings::port(config, OAUTH_REDIRECT_PORT)?;
        let loopback = Loopback::bind(wanted).await?;
        if wanted.is_some() && loopback.port() != wanted {
            return Err(
                AppError::new(AppErrorCode::Conflict, copy::REDIRECT_PORT_BUSY)
                    .with_detail("field", format!("config.{OAUTH_REDIRECT_PORT}")),
            );
        }
        let client = OAuthClient {
            id: &client_id,
            secret: secret.as_ref().map(Plaintext::expose),
        };
        let pkce = Pkce::generate();
        let state = random_state();
        let redirect_uri = loopback.redirect_uri.clone();
        let address = oauth::authorize_url(&preset, &client, &redirect_uri, &state, &pkce)?;
        (request.opener)(&address)?;
        let code = loopback
            .wait_for_code(&state, SIGN_IN_TIMEOUT, request.cancel)
            .await?;
        let grant = oauth::exchange(
            &self.http,
            CodeExchange {
                provider: preset.key,
                token_endpoint: self.token_endpoint(&preset),
                client,
                code: &code,
                redirect_uri: &redirect_uri,
                verifier: &pkce.verifier,
                now: self.clock.now(),
            },
        )
        .await?;
        Ok(SignedIn {
            field: OAUTH_TOKEN.to_owned(),
            value: Plaintext::new(grant.encode()),
        })
    }

    async fn handled(
        &self,
        _account: &IntegrationAccountDto,
        config: &IntegrationConfigView,
        event: &IntegrationHandled,
    ) -> Result<(), AppError> {
        if event.trigger != RECEIVED {
            return Ok(());
        }
        let settings = self.settings(config).await?;
        if !settings.mark_read {
            return Ok(());
        }
        let Ok(message) = serde_json::from_value::<MailMessageDto>(event.payload.clone()) else {
            return Ok(());
        };
        let reference = message.reference;
        let mut imap = self.connect(&settings).await?;
        let outcome = async {
            let mailbox = imap.select(&reference.folder).await?;
            if mailbox.uid_validity != reference.uid_validity {
                return Ok(());
            }
            imap.mark_seen(reference.uid).await
        }
        .await;
        imap.logout().await;
        outcome
    }
}

#[cfg(test)]
mod server;
#[cfg(test)]
mod tests;
