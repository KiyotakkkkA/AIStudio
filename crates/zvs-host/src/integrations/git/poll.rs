use std::collections::BTreeMap;
use std::time::Duration;

use serde_json::{Value, json};
use zvs_contract::{AppError, Timestamp, UntrustedTextDto};

use super::client::{ForgeClient, ForgeError};
use super::forge::{
    Comment, Forge, Item, branches, github_comment, gitlab_note_event, iso, item, project,
};
use crate::integrations::common::clip;
use crate::integrations::{IntegrationEvent, IntegrationSession};

pub const ISSUE_OPENED: &str = "issueOpened";
pub const ISSUE_COMMENTED: &str = "issueCommented";
pub const PULL_OPENED: &str = "pullRequestOpened";
pub const PULL_UPDATED: &str = "pullRequestUpdated";
pub const PUSH: &str = "push";
pub const BODY_MAX: usize = 8000;
pub const PAGE: u32 = 50;
const DAY_MS: Timestamp = 86_400_000;

pub fn untrusted(text: &str) -> UntrustedTextDto {
    UntrustedTextDto::new(clip(text, BODY_MAX))
}

pub fn item_payload(repository: &str, item: &Item) -> Value {
    json!({
        "untrusted": true,
        "repository": repository,
        "number": item.number,
        "title": untrusted(&item.title),
        "body": untrusted(&item.body),
        "state": item.state,
        "author": item.author,
        "url": item.url,
        "labels": item.labels,
        "head": item.head,
        "base": item.base,
        "createdAt": item.created_at,
        "updatedAt": item.updated_at,
    })
}

pub fn comment_payload(repository: &str, comment: &Comment) -> Value {
    json!({
        "untrusted": true,
        "repository": repository,
        "number": comment.number,
        "onPullRequest": comment.pull_request,
        "commentId": comment.id,
        "body": untrusted(&comment.body),
        "author": comment.author,
        "url": comment.url,
        "createdAt": comment.created_at,
    })
}

pub struct Poller<'a> {
    pub forge: Forge,
    pub client: &'a ForgeClient<'a>,
    pub session: &'a IntegrationSession,
    pub now: Timestamp,
}

impl Poller<'_> {
    fn trigger(&self, action: &str) -> String {
        self.forge.name(action)
    }

    fn emit(&self, action: &str, event_id: String, payload: Value) -> Result<(), AppError> {
        self.session.emit(IntegrationEvent::new(
            self.trigger(action),
            event_id,
            payload,
        ))?;
        Ok(())
    }

    fn since(&self, key: &str) -> Result<Option<Timestamp>, AppError> {
        Ok(self
            .session
            .cursor(key)?
            .and_then(|raw| raw.parse::<Timestamp>().ok()))
    }

    fn baseline(&self, key: &str) -> Result<Option<Timestamp>, AppError> {
        match self.since(key)? {
            Some(since) => Ok(Some(since)),
            None => {
                self.session.set_cursor(key, &self.now.to_string())?;
                Ok(None)
            }
        }
    }

    fn advance(
        &self,
        key: &str,
        since: Timestamp,
        seen: impl Iterator<Item = Timestamp>,
    ) -> Result<(), AppError> {
        let latest = seen.fold(since, Timestamp::max);
        if latest > since {
            self.session.set_cursor(key, &latest.to_string())?;
        }
        Ok(())
    }

    fn deliver_items(
        &self,
        repository: &str,
        since: Timestamp,
        items: &[Item],
    ) -> Result<(), AppError> {
        for item in items.iter().filter(|item| item.updated_at >= since) {
            let opened = item.created_at >= since;
            let payload = item_payload(repository, item);
            match (item.pull_request, opened) {
                (false, true) => self.emit(
                    ISSUE_OPENED,
                    format!("{repository}#issue:{}:opened", item.number),
                    payload,
                )?,
                (false, false) => {}
                (true, true) => self.emit(
                    PULL_OPENED,
                    format!("{repository}#pull:{}:opened", item.number),
                    payload,
                )?,
                (true, false) => self.emit(
                    PULL_UPDATED,
                    format!("{repository}#pull:{}:{}", item.number, item.updated_at),
                    payload,
                )?,
            }
        }
        Ok(())
    }

    async fn items(
        &self,
        repository: &str,
        key: &str,
        path: String,
        pulls: bool,
    ) -> Result<(), ForgeError> {
        let Some(since) = self.baseline(key)? else {
            return Ok(());
        };
        let separator = if path.contains('?') { '&' } else { '?' };
        let filter = match self.forge {
            Forge::GitHub => format!("since={}", iso(since)),
            Forge::GitLab => format!("updated_after={}", iso(since)),
        };
        let Some(body) = self
            .client
            .get_changed(&format!("{path}{separator}{filter}"))
            .await?
        else {
            return Ok(());
        };
        let items: Vec<Item> = body
            .as_array()
            .into_iter()
            .flatten()
            .map(|value| item(self.forge, value, pulls))
            .collect();
        self.deliver_items(repository, since, &items)?;
        self.advance(key, since, items.iter().map(|item| item.updated_at))?;
        Ok(())
    }

    async fn comments(&self, repository: &str) -> Result<(), ForgeError> {
        let key = format!("{repository}:comments");
        let Some(since) = self.baseline(&key)? else {
            return Ok(());
        };
        let root = project(self.forge, repository);
        let path = match self.forge {
            Forge::GitHub => format!(
                "{root}/issues/comments?sort=updated&direction=asc&per_page={PAGE}&since={}",
                iso(since)
            ),
            Forge::GitLab => format!(
                "{root}/events?action=commented&sort=asc&per_page={PAGE}&after={}",
                &iso(since - DAY_MS)[..10]
            ),
        };
        let Some(body) = self.client.get_changed(&path).await? else {
            return Ok(());
        };
        let comments: Vec<Comment> = body
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|value| match self.forge {
                Forge::GitHub => Some(github_comment(value)),
                Forge::GitLab => gitlab_note_event(value),
            })
            .collect();
        for comment in comments
            .iter()
            .filter(|comment| comment.created_at >= since)
        {
            self.emit(
                ISSUE_COMMENTED,
                format!("{repository}#comment:{}", comment.id),
                comment_payload(repository, comment),
            )?;
        }
        self.advance(
            &key,
            since,
            comments.iter().map(|comment| comment.updated_at),
        )?;
        Ok(())
    }

    async fn pushes(&self, repository: &str) -> Result<(), ForgeError> {
        let key = format!("{repository}:branches");
        let root = project(self.forge, repository);
        let path = match self.forge {
            Forge::GitHub => format!("{root}/branches?per_page=100"),
            Forge::GitLab => format!("{root}/repository/branches?per_page=100"),
        };
        let Some(body) = self.client.get_changed(&path).await? else {
            return Ok(());
        };
        let current: BTreeMap<String, String> = branches(self.forge, &body).into_iter().collect();
        let known: Option<BTreeMap<String, String>> = self
            .session
            .cursor(&key)?
            .and_then(|raw| serde_json::from_str(&raw).ok());
        if let Some(known) = &known {
            for (branch, sha) in &current {
                let before = known.get(branch);
                if before == Some(sha) {
                    continue;
                }
                self.emit(
                    PUSH,
                    format!("{repository}#push:{branch}:{sha}"),
                    json!({
                        "untrusted": true,
                        "repository": repository,
                        "branch": branch,
                        "before": before,
                        "after": sha,
                    }),
                )?;
            }
        }
        if known.as_ref() != Some(&current) {
            self.session
                .set_cursor(&key, &serde_json::to_string(&current).unwrap_or_default())?;
        }
        Ok(())
    }

    pub async fn repository(&self, repository: &str) -> Result<(), ForgeError> {
        let root = project(self.forge, repository);
        match self.forge {
            Forge::GitHub => {
                self.items(
                    repository,
                    &format!("{repository}:items"),
                    format!("{root}/issues?state=all&sort=updated&direction=asc&per_page={PAGE}"),
                    false,
                )
                .await?;
            }
            Forge::GitLab => {
                self.items(
                    repository,
                    &format!("{repository}:issues"),
                    format!("{root}/issues?scope=all&order_by=updated_at&sort=asc&per_page={PAGE}"),
                    false,
                )
                .await?;
                self.items(
                    repository,
                    &format!("{repository}:pulls"),
                    format!(
                        "{root}/merge_requests?scope=all&order_by=updated_at&sort=asc&per_page={PAGE}"
                    ),
                    true,
                )
                .await?;
            }
        }
        self.comments(repository).await?;
        self.pushes(repository).await
    }
}

pub fn wait_for(seconds: u64) -> Duration {
    Duration::from_secs(seconds.max(1))
}
