use serde_json::json;
use zvs_contract::{AppError, IntegrationActivityLevel, Timestamp};

use super::imap::{ImapConnection, Woken};
use super::message::{event_id, summary};
use super::settings::MailSettings;
use super::tls::MailTrust;
use crate::integrations::{IntegrationEvent, IntegrationSession};
use crate::platform::clock::SharedClock;
use crate::scenarios::template::civil_date;
use crate::workers::WorkerFault;

pub const RECEIVED: &str = "mail.received";
pub const BATCH: usize = 200;
const DAY_MS: Timestamp = 86_400_000;
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

pub fn imap_date(timestamp: Timestamp) -> String {
    let civil = civil_date(timestamp.max(0));
    let mut parts = civil.split('.');
    let day = parts.next().unwrap_or("01").trim_start_matches('0');
    let month = parts
        .next()
        .and_then(|month| month.parse::<usize>().ok())
        .and_then(|month| MONTHS.get(month.wrapping_sub(1)))
        .copied()
        .unwrap_or("Jan");
    let year = parts.next().unwrap_or("1970");
    format!("{day}-{month}-{year}")
}

pub fn cursor_key(folder: &str) -> String {
    format!("uid:{folder}")
}

pub fn parse_cursor(value: &str) -> Option<(u32, u32)> {
    let (validity, uid) = value.split_once(':')?;
    Some((validity.parse().ok()?, uid.parse().ok()?))
}

pub struct Sync<'a> {
    pub session: &'a IntegrationSession,
    pub settings: &'a MailSettings,
    pub clock: &'a SharedClock,
}

impl Sync<'_> {
    fn horizon(&self) -> Timestamp {
        let days = i64::try_from(self.settings.ignore_older_days).unwrap_or(i64::MAX);
        self.clock.now().saturating_sub(days.saturating_mul(DAY_MS))
    }

    async fn folder(&self, imap: &mut ImapConnection, folder: &str) -> Result<(), AppError> {
        let mailbox = imap.examine(folder).await?;
        let key = cursor_key(folder);
        let stored = self
            .session
            .cursor(&key)?
            .as_deref()
            .and_then(parse_cursor)
            .filter(|(validity, _)| *validity == mailbox.uid_validity);
        let horizon = self.horizon();
        let mut uids = match stored {
            Some((_, last)) => imap
                .search(&format!("UID {}:*", last.saturating_add(1)))
                .await?
                .into_iter()
                .filter(|uid| *uid > last)
                .collect(),
            None => {
                imap.search(&format!("UNSEEN SINCE {}", imap_date(horizon)))
                    .await?
            }
        };
        uids.truncate(BATCH);
        let mut highest = stored.map_or(0, |(_, last)| last);
        for fetched in imap.fetch(folder, mailbox.uid_validity, &uids).await? {
            let message = summary(&fetched);
            let uid = message.reference.uid;
            if message.date.is_none_or(|date| date >= horizon) {
                let event = IntegrationEvent::new(RECEIVED, event_id(&message), json!(message));
                if let Err(error) = self.session.emit(event) {
                    self.session
                        .activity(IntegrationActivityLevel::Warn, &error.message);
                }
            }
            highest = highest.max(uid);
            self.session
                .set_cursor(&key, &format!("{}:{highest}", mailbox.uid_validity))?;
        }
        if stored.is_none() {
            let baseline = mailbox
                .uid_next
                .map_or(highest, |next| highest.max(next.saturating_sub(1)));
            self.session
                .set_cursor(&key, &format!("{}:{baseline}", mailbox.uid_validity))?;
        }
        Ok(())
    }

    async fn round(&self, imap: &mut ImapConnection) -> Result<(), AppError> {
        for folder in &self.settings.folders {
            self.folder(imap, folder).await?;
        }
        self.session.worker.heartbeat();
        Ok(())
    }

    pub async fn run(&self, trust: &MailTrust) -> Result<(), WorkerFault> {
        let mut imap = ImapConnection::connect(&self.settings.imap, trust).await?;
        let idle = self.settings.folders.len() == 1 && imap.supports_idle().await;
        self.round(&mut imap).await?;
        self.session.worker.ready();
        loop {
            if idle {
                let (connection, woken) = imap.idle(self.session.cancellation()).await?;
                imap = connection;
                if matches!(woken, Woken::Cancelled) {
                    imap.logout().await;
                    return Ok(());
                }
            } else if !self.session.worker.sleep(self.settings.poll).await {
                imap.logout().await;
                return Ok(());
            }
            self.round(&mut imap).await?;
        }
    }
}
