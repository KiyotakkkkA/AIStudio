use rusqlite::Row;
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use zvs_contract::Timestamp;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GrantKind {
    Folder,
    Site,
    App,
    Tool,
}

impl GrantKind {
    pub const ALL: [Self; 4] = [Self::Folder, Self::Site, Self::App, Self::Tool];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Folder => "folder",
            Self::Site => "site",
            Self::App => "app",
            Self::Tool => "tool",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GrantDecision {
    Allow,
    Ask,
    Block,
}

impl GrantDecision {
    pub const ALL: [Self; 3] = [Self::Allow, Self::Ask, Self::Block];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Ask => "ask",
            Self::Block => "block",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|decision| decision.as_str() == value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GrantScope {
    Conversation,
    Always,
}

impl GrantScope {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Conversation => "conversation",
            Self::Always => "always",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [Self::Conversation, Self::Always]
            .into_iter()
            .find(|scope| scope.as_str() == value)
    }

    pub const fn of(owner: Option<&str>) -> Self {
        if owner.is_some() {
            Self::Conversation
        } else {
            Self::Always
        }
    }
}

macro_rules! text_column {
    ($type:ty) => {
        impl FromSql for $type {
            fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
                let text = value.as_str()?;
                Self::parse(text)
                    .ok_or_else(|| FromSqlError::Other(format!("unknown value «{text}»").into()))
            }
        }

        impl ToSql for $type {
            fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
                Ok(ToSqlOutput::from(self.as_str()))
            }
        }
    };
}

text_column!(GrantKind);
text_column!(GrantDecision);
text_column!(GrantScope);

pub const GRANTED_BY_USER: &str = "user";
pub const GRANTED_BY_CLI: &str = "cli-confirmed";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessGrantRow {
    pub id: String,
    pub resource_kind: GrantKind,
    pub resource: String,
    pub action: String,
    pub decision: GrantDecision,
    pub scope: GrantScope,
    pub owner: Option<String>,
    pub granted_by: String,
    pub granted_at: Timestamp,
    pub used_at: Option<Timestamp>,
    pub detail: Option<String>,
}

impl AccessGrantRow {
    pub const COLUMNS: &str = "id, resource_kind, resource, action, decision, scope, owner, \
         granted_by, granted_at, used_at, detail";

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            resource_kind: row.get("resource_kind")?,
            resource: row.get("resource")?,
            action: row.get("action")?,
            decision: row.get("decision")?,
            scope: row.get("scope")?,
            owner: row.get("owner")?,
            granted_by: row.get("granted_by")?,
            granted_at: row.get("granted_at")?,
            used_at: row.get("used_at")?,
            detail: row.get("detail")?,
        })
    }
}
