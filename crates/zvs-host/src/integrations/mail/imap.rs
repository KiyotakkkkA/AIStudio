use std::time::Duration;

use async_imap::extensions::idle::IdleResponse;
use async_imap::types::{Fetch, Flag};
use async_imap::{Client, Session};
use futures::TryStreamExt;
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_util::sync::CancellationToken;
use zvs_contract::{AppError, AppErrorCode, MailRefDto};

use super::copy;
use super::message::Fetched;
use super::oauth::XOAuth2;
use super::settings::{Auth, Endpoint, Security};
use super::tls::MailTrust;

pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
pub const COMMAND_TIMEOUT: Duration = Duration::from_secs(120);
pub const IDLE_REFRESH: Duration = Duration::from_secs(25 * 60);
pub const FULL_FETCH_MAX: u32 = 2 * 1024 * 1024;
const FETCH_BATCH: usize = 50;

type Stream = TlsStream<TcpStream>;

pub struct Mailbox {
    pub uid_validity: u32,
    pub uid_next: Option<u32>,
}

pub enum Woken {
    Changed,
    Quiet,
    Cancelled,
}

pub struct ImapConnection {
    session: Session<Stream>,
}

fn imap_error(_: async_imap::error::Error) -> AppError {
    AppError::new(AppErrorCode::ProviderUnreachable, copy::IMAP_FAILED)
}

fn timed_out() -> AppError {
    AppError::new(AppErrorCode::ProviderUnreachable, copy::TIMEOUT)
}

async fn within<T>(
    limit: Duration,
    work: impl Future<Output = Result<T, AppError>>,
) -> Result<T, AppError> {
    tokio::time::timeout(limit, work)
        .await
        .unwrap_or_else(|_| Err(timed_out()))
}

pub fn uid_set(uids: &[u32]) -> String {
    uids.iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

pub fn quote(value: &str) -> String {
    let escaped: String = value
        .chars()
        .filter(|character| !matches!(character, '\r' | '\n'))
        .flat_map(|character| match character {
            '"' | '\\' => vec!['\\', character],
            other => vec![other],
        })
        .collect();
    format!("\"{escaped}\"")
}

async fn open(endpoint: &Endpoint, trust: &MailTrust) -> Result<Stream, AppError> {
    let tcp = TcpStream::connect((endpoint.host.as_str(), endpoint.port))
        .await
        .map_err(|_| AppError::new(AppErrorCode::ProviderUnreachable, copy::CONNECT_FAILED))?;
    match endpoint.security {
        Security::Tls => trust.wrap(&endpoint.host, tcp).await,
        Security::StartTls => {
            let mut client = Client::new(tcp);
            client
                .read_response()
                .await
                .map_err(|_| AppError::new(AppErrorCode::ProviderUnreachable, copy::IMAP_FAILED))?
                .ok_or_else(|| {
                    AppError::new(AppErrorCode::ProviderUnreachable, copy::IMAP_FAILED)
                })?;
            client
                .run_command_and_check_ok("STARTTLS", None)
                .await
                .map_err(|_| {
                    AppError::new(AppErrorCode::ProviderUnreachable, copy::STARTTLS_REFUSED)
                })?;
            trust.wrap(&endpoint.host, client.into_inner()).await
        }
    }
}

impl ImapConnection {
    pub async fn connect(endpoint: &Endpoint, trust: &MailTrust) -> Result<Self, AppError> {
        within(CONNECT_TIMEOUT, async {
            let stream = open(endpoint, trust).await?;
            let mut client = Client::new(stream);
            if endpoint.security == Security::Tls {
                client
                    .read_response()
                    .await
                    .map_err(|_| {
                        AppError::new(AppErrorCode::ProviderUnreachable, copy::IMAP_FAILED)
                    })?
                    .ok_or_else(|| {
                        AppError::new(AppErrorCode::ProviderUnreachable, copy::IMAP_FAILED)
                    })?;
            }
            let session = match &endpoint.auth {
                Auth::Password(password) => client.login(&endpoint.user, password.expose()).await,
                Auth::Bearer(token) => {
                    client
                        .authenticate("XOAUTH2", XOAuth2::new(&endpoint.user, token.expose()))
                        .await
                }
            }
            .map_err(|_| AppError::new(AppErrorCode::ProviderAuthFailed, copy::LOGIN_FAILED))?;
            Ok(Self { session })
        })
        .await
    }

    pub async fn folder_count(&mut self) -> Result<usize, AppError> {
        within(COMMAND_TIMEOUT, async {
            let names: Vec<_> = self
                .session
                .list(None, Some("*"))
                .await
                .map_err(imap_error)?
                .try_collect()
                .await
                .map_err(imap_error)?;
            Ok(names.len())
        })
        .await
    }

    pub async fn supports_idle(&mut self) -> bool {
        within(COMMAND_TIMEOUT, async {
            self.session.capabilities().await.map_err(imap_error)
        })
        .await
        .is_ok_and(|capabilities| capabilities.has_str("IDLE"))
    }

    async fn open_folder(&mut self, folder: &str, writable: bool) -> Result<Mailbox, AppError> {
        within(COMMAND_TIMEOUT, async {
            let mailbox = if writable {
                self.session.select(folder).await
            } else {
                self.session.examine(folder).await
            }
            .map_err(|error| match error {
                async_imap::error::Error::No(_) | async_imap::error::Error::Bad(_) => {
                    AppError::new(AppErrorCode::NotFound, copy::FOLDER_MISSING)
                        .with_detail("folder", folder)
                }
                other => imap_error(other),
            })?;
            Ok(Mailbox {
                uid_validity: mailbox.uid_validity.unwrap_or(0),
                uid_next: mailbox.uid_next,
            })
        })
        .await
    }

    pub async fn examine(&mut self, folder: &str) -> Result<Mailbox, AppError> {
        self.open_folder(folder, false).await
    }

    pub async fn select(&mut self, folder: &str) -> Result<Mailbox, AppError> {
        self.open_folder(folder, true).await
    }

    pub async fn search(&mut self, query: &str) -> Result<Vec<u32>, AppError> {
        within(COMMAND_TIMEOUT, async {
            let mut uids: Vec<u32> = self
                .session
                .uid_search(query)
                .await
                .map_err(imap_error)?
                .into_iter()
                .collect();
            uids.sort_unstable();
            Ok(uids)
        })
        .await
    }

    async fn fetch_raw(&mut self, uids: &[u32], query: &str) -> Result<Vec<Fetch>, AppError> {
        within(COMMAND_TIMEOUT, async {
            self.session
                .uid_fetch(uid_set(uids), query)
                .await
                .map_err(imap_error)?
                .try_collect()
                .await
                .map_err(imap_error)
        })
        .await
    }

    pub async fn fetch(
        &mut self,
        folder: &str,
        uid_validity: u32,
        uids: &[u32],
    ) -> Result<Vec<Fetched>, AppError> {
        let mut fetched = Vec::new();
        for batch in uids.chunks(FETCH_BATCH) {
            let sizes = self.fetch_raw(batch, "(UID RFC822.SIZE)").await?;
            let mut small = Vec::new();
            let mut large = Vec::new();
            for item in &sizes {
                if let Some(uid) = item.uid {
                    if item.size.unwrap_or(0) <= FULL_FETCH_MAX {
                        small.push(uid);
                    } else {
                        large.push(uid);
                    }
                }
            }
            let mut items = Vec::new();
            if !small.is_empty() {
                items.extend(
                    self.fetch_raw(&small, "(UID FLAGS INTERNALDATE RFC822.SIZE BODY.PEEK[])")
                        .await?,
                );
            }
            if !large.is_empty() {
                items.extend(
                    self.fetch_raw(
                        &large,
                        "(UID FLAGS INTERNALDATE RFC822.SIZE BODY.PEEK[HEADER])",
                    )
                    .await?,
                );
            }
            for item in items {
                let Some(uid) = item.uid else {
                    continue;
                };
                let raw = item
                    .body()
                    .or_else(|| item.header())
                    .map(<[u8]>::to_vec)
                    .unwrap_or_default();
                fetched.push(Fetched {
                    reference: MailRefDto {
                        folder: folder.to_owned(),
                        uid_validity,
                        uid,
                    },
                    raw,
                    size: u64::from(item.size.unwrap_or(0)),
                    seen: item.flags().any(|flag| matches!(flag, Flag::Seen)),
                    internal_date: item
                        .internal_date()
                        .map(|date| date.timestamp_millis())
                        .filter(|millis| *millis >= 0),
                });
            }
        }
        fetched.sort_by_key(|item| item.reference.uid);
        Ok(fetched)
    }

    pub async fn mark_seen(&mut self, uid: u32) -> Result<(), AppError> {
        within(COMMAND_TIMEOUT, async {
            let _: Vec<_> = self
                .session
                .uid_store(uid.to_string(), "+FLAGS.SILENT (\\Seen)")
                .await
                .map_err(imap_error)?
                .try_collect()
                .await
                .map_err(imap_error)?;
            Ok(())
        })
        .await
    }

    pub async fn idle(self, cancel: &CancellationToken) -> Result<(Self, Woken), AppError> {
        let mut handle = self.session.idle();
        within(COMMAND_TIMEOUT, async {
            handle.init().await.map_err(imap_error)
        })
        .await?;
        let woken = {
            let (wait, _stop) = handle.wait_with_timeout(IDLE_REFRESH);
            tokio::select! {
                outcome = wait => match outcome.map_err(imap_error)? {
                    IdleResponse::NewData(_) => Woken::Changed,
                    IdleResponse::Timeout | IdleResponse::ManualInterrupt => Woken::Quiet,
                },
                () = cancel.cancelled() => Woken::Cancelled,
            }
        };
        let session = within(COMMAND_TIMEOUT, async {
            handle.done().await.map_err(imap_error)
        })
        .await?;
        Ok((Self { session }, woken))
    }

    pub async fn logout(mut self) {
        let _ = tokio::time::timeout(CONNECT_TIMEOUT, self.session.logout()).await;
    }
}
