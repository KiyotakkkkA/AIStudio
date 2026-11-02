use serde_json::{Value, json};
use zvs_contract::{AppError, AppErrorCode, IntegrationKind, Timestamp};

use super::copy;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Forge {
    GitHub,
    GitLab,
}

pub const GITHUB_API: &str = "https://api.github.com";
pub const GITHUB_WEB: &str = "https://github.com";
pub const GITLAB_WEB: &str = "https://gitlab.com";

impl Forge {
    pub const fn kind(self) -> IntegrationKind {
        match self {
            Self::GitHub => IntegrationKind::Github,
            Self::GitLab => IntegrationKind::Gitlab,
        }
    }

    pub const fn prefix(self) -> &'static str {
        match self {
            Self::GitHub => "github",
            Self::GitLab => "gitlab",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::GitHub => copy::GITHUB_LABEL,
            Self::GitLab => copy::GITLAB_LABEL,
        }
    }

    pub const fn default_base(self) -> &'static str {
        match self {
            Self::GitHub => GITHUB_API,
            Self::GitLab => GITLAB_WEB,
        }
    }

    pub fn api_root(self, base: &str) -> String {
        let base = base.trim_end_matches('/');
        match self {
            Self::GitHub => base.to_owned(),
            Self::GitLab => format!("{base}/api/v4"),
        }
    }

    pub fn name(self, action: &str) -> String {
        format!("{}.{action}", self.prefix())
    }
}

pub fn encode(text: &str) -> String {
    let mut encoded = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn segment_ok(segment: &str) -> bool {
    !segment.is_empty()
        && segment != "."
        && segment != ".."
        && segment
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

pub fn repository(forge: Forge, raw: &str) -> Result<String, AppError> {
    let trimmed = raw.trim().trim_matches('/');
    let segments: Vec<&str> = trimmed.split('/').collect();
    let valid = match forge {
        Forge::GitHub => segments.len() == 2,
        Forge::GitLab => segments.len() >= 2,
    } && segments.iter().all(|segment| segment_ok(segment));
    if valid {
        Ok(trimmed.to_owned())
    } else {
        Err(AppError::new(
            AppErrorCode::ValidationFailed,
            match forge {
                Forge::GitHub => copy::BAD_REPOSITORY,
                Forge::GitLab => copy::BAD_GITLAB_REPOSITORY,
            },
        )
        .with_detail("field", "repository"))
    }
}

pub fn project(forge: Forge, repository: &str) -> String {
    match forge {
        Forge::GitHub => format!("/repos/{repository}"),
        Forge::GitLab => format!("/projects/{}", encode(repository)),
    }
}

pub fn file_path(raw: &str) -> String {
    raw.trim().trim_matches('/').to_owned()
}

pub fn iso(at: Timestamp) -> String {
    let full = crate::platform::paths::utc_iso(at);
    format!("{}Z", &full[..19])
}

pub fn millis(value: &Value) -> Timestamp {
    value
        .as_str()
        .and_then(zvs_copilot::iso_millis)
        .unwrap_or_default()
}

fn text(value: &Value) -> String {
    value.as_str().unwrap_or_default().to_owned()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub number: u64,
    pub title: String,
    pub body: String,
    pub state: String,
    pub author: String,
    pub url: String,
    pub labels: Vec<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub pull_request: bool,
    pub head: Option<String>,
    pub base: Option<String>,
}

pub fn item(forge: Forge, value: &Value, pull_request: bool) -> Item {
    match forge {
        Forge::GitHub => Item {
            number: value["number"].as_u64().unwrap_or_default(),
            title: text(&value["title"]),
            body: text(&value["body"]),
            state: text(&value["state"]),
            author: text(&value["user"]["login"]),
            url: text(&value["html_url"]),
            labels: value["labels"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|label| label["name"].as_str().map(str::to_owned))
                .collect(),
            created_at: millis(&value["created_at"]),
            updated_at: millis(&value["updated_at"]),
            pull_request: pull_request || value.get("pull_request").is_some(),
            head: value["head"]["ref"].as_str().map(str::to_owned),
            base: value["base"]["ref"].as_str().map(str::to_owned),
        },
        Forge::GitLab => Item {
            number: value["iid"].as_u64().unwrap_or_default(),
            title: text(&value["title"]),
            body: text(&value["description"]),
            state: text(&value["state"]),
            author: text(&value["author"]["username"]),
            url: text(&value["web_url"]),
            labels: value["labels"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|label| label.as_str().map(str::to_owned))
                .collect(),
            created_at: millis(&value["created_at"]),
            updated_at: millis(&value["updated_at"]),
            pull_request,
            head: value["source_branch"].as_str().map(str::to_owned),
            base: value["target_branch"].as_str().map(str::to_owned),
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    pub id: u64,
    pub number: u64,
    pub body: String,
    pub author: String,
    pub url: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub pull_request: bool,
}

pub fn github_comment(value: &Value) -> Comment {
    let number = value["issue_url"]
        .as_str()
        .and_then(|url| url.rsplit('/').next())
        .and_then(|number| number.parse().ok())
        .unwrap_or_default();
    let url = value["html_url"].as_str().map(str::to_owned);
    Comment {
        id: value["id"].as_u64().unwrap_or_default(),
        number,
        body: text(&value["body"]),
        author: text(&value["user"]["login"]),
        pull_request: url.as_deref().is_some_and(|url| url.contains("/pull/")),
        url,
        created_at: millis(&value["created_at"]),
        updated_at: millis(&value["updated_at"]),
    }
}

pub fn gitlab_note_event(value: &Value) -> Option<Comment> {
    let note = value.get("note")?;
    let kind = note["noteable_type"].as_str()?;
    if kind != "Issue" && kind != "MergeRequest" {
        return None;
    }
    let created_at = millis(&value["created_at"]);
    Some(Comment {
        id: note["id"].as_u64()?,
        number: note["noteable_iid"].as_u64()?,
        body: text(&note["body"]),
        author: text(&value["author"]["username"]),
        url: None,
        created_at,
        updated_at: created_at,
        pull_request: kind == "MergeRequest",
    })
}

pub fn branches(forge: Forge, value: &Value) -> Vec<(String, String)> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|branch| {
            let name = branch["name"].as_str()?.to_owned();
            let sha = match forge {
                Forge::GitHub => branch["commit"]["sha"].as_str(),
                Forge::GitLab => branch["commit"]["id"].as_str(),
            }?;
            Some((name, sha.to_owned()))
        })
        .collect()
}

pub fn issue_body(forge: Forge, title: &str, body: &str, labels: &[String]) -> Value {
    match forge {
        Forge::GitHub => json!({ "title": title, "body": body, "labels": labels }),
        Forge::GitLab => json!({ "title": title, "description": body, "labels": labels.join(",") }),
    }
}

pub fn pull_body(
    forge: Forge,
    title: &str,
    head: &str,
    base: &str,
    body: &str,
    draft: bool,
) -> Value {
    match forge {
        Forge::GitHub => {
            json!({ "title": title, "head": head, "base": base, "body": body, "draft": draft })
        }
        Forge::GitLab => {
            let title = if draft {
                format!("Draft: {title}")
            } else {
                title.to_owned()
            };
            json!({
                "title": title,
                "source_branch": head,
                "target_branch": base,
                "description": body,
            })
        }
    }
}
