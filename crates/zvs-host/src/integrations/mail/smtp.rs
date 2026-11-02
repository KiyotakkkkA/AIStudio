use std::time::Duration;

use lettre::message::header::ContentType;
use lettre::message::{Mailbox, Message};
use lettre::transport::smtp::authentication::{Credentials, Mechanism};
use lettre::transport::smtp::client::Tls;
use lettre::{AsyncSmtpTransport, AsyncTransport, Tokio1Executor};
use zvs_contract::{AppError, AppErrorCode, MailDraftDto};

use super::copy;
use super::settings::{Auth, Endpoint, Security};
use super::tls::MailTrust;

pub const SMTP_TIMEOUT: Duration = Duration::from_secs(60);

fn transport(
    endpoint: &Endpoint,
    trust: &MailTrust,
) -> Result<AsyncSmtpTransport<Tokio1Executor>, AppError> {
    let parameters = trust.smtp(&endpoint.host)?;
    let tls = match endpoint.security {
        Security::Tls => Tls::Wrapper(parameters),
        Security::StartTls => Tls::Required(parameters),
    };
    let (secret, mechanisms) = match &endpoint.auth {
        Auth::Password(password) => (password.expose(), vec![Mechanism::Plain, Mechanism::Login]),
        Auth::Bearer(token) => (token.expose(), vec![Mechanism::Xoauth2]),
    };
    Ok(
        AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(endpoint.host.as_str())
            .port(endpoint.port)
            .tls(tls)
            .credentials(Credentials::new(endpoint.user.clone(), secret.to_owned()))
            .authentication(mechanisms)
            .timeout(Some(SMTP_TIMEOUT))
            .build(),
    )
}

fn failure(error: &lettre::transport::smtp::Error) -> AppError {
    if error.is_timeout() {
        AppError::new(AppErrorCode::ProviderUnreachable, copy::TIMEOUT)
    } else if error.is_tls() {
        AppError::new(AppErrorCode::ProviderUnreachable, copy::TLS_FAILED)
    } else if error
        .status()
        .is_some_and(|code| code.to_string().starts_with("535"))
    {
        AppError::new(AppErrorCode::ProviderAuthFailed, copy::LOGIN_FAILED)
    } else if error.is_permanent() || error.is_transient() {
        AppError::new(AppErrorCode::ProviderUnreachable, copy::SMTP_FAILED)
    } else {
        AppError::new(AppErrorCode::ProviderUnreachable, copy::CONNECT_FAILED)
    }
}

pub async fn probe(endpoint: &Endpoint, trust: &MailTrust) -> Result<(), AppError> {
    let transport = transport(endpoint, trust)?;
    match tokio::time::timeout(SMTP_TIMEOUT, transport.test_connection()).await {
        Ok(Ok(true)) => Ok(()),
        Ok(Ok(false)) => Err(AppError::new(
            AppErrorCode::ProviderUnreachable,
            copy::CONNECT_FAILED,
        )),
        Ok(Err(error)) => Err(failure(&error)),
        Err(_) => Err(AppError::new(
            AppErrorCode::ProviderUnreachable,
            copy::TIMEOUT,
        )),
    }
}

pub fn mailbox(value: &str) -> Result<Mailbox, AppError> {
    value.trim().parse::<Mailbox>().map_err(|_| {
        AppError::new(AppErrorCode::ValidationFailed, copy::BAD_ADDRESS)
            .with_detail("address", value)
    })
}

pub fn compose(draft: &MailDraftDto, message_id: &str) -> Result<Message, AppError> {
    let mut builder = Message::builder()
        .from(mailbox(&draft.from)?)
        .subject(draft.subject.clone())
        .message_id(Some(message_id.to_owned()))
        .date_now()
        .header(ContentType::TEXT_PLAIN);
    for recipient in &draft.to {
        builder = builder.to(mailbox(recipient)?);
    }
    for recipient in &draft.cc {
        builder = builder.cc(mailbox(recipient)?);
    }
    if let Some(parent) = &draft.in_reply_to {
        builder = builder.in_reply_to(parent.clone());
    }
    if !draft.references.is_empty() {
        builder = builder.references(draft.references.join(" "));
    }
    builder
        .body(draft.text.clone())
        .map_err(|_| AppError::new(AppErrorCode::ValidationFailed, copy::BAD_ADDRESS))
}

pub async fn send(
    endpoint: &Endpoint,
    trust: &MailTrust,
    message: Message,
) -> Result<(), AppError> {
    let transport = transport(endpoint, trust)?;
    match tokio::time::timeout(SMTP_TIMEOUT, transport.send(message)).await {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(error)) => Err(failure(&error)),
        Err(_) => Err(AppError::new(
            AppErrorCode::ProviderUnreachable,
            copy::TIMEOUT,
        )),
    }
}
