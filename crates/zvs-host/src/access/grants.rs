use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use zvs_contract::{AppError, ToolGrantDto, ToolTier};

use super::request::{AccessRequest, Resource};
use crate::data::schema::{AccessGrantRow, GrantDecision, GrantKind, GrantScope};
use crate::data::{Database, Repositories};
use crate::platform::clock::SharedClock;
use crate::platform::ids::new_id;
use crate::workspace::paths::covers as folder_covers;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GrantQuery<'a> {
    pub kind: GrantKind,
    pub resource: &'a str,
    pub action: &'a str,
    pub owner: Option<&'a str>,
    pub run: Option<&'a str>,
}

impl<'a> GrantQuery<'a> {
    pub const fn new(kind: GrantKind, resource: &'a str) -> Self {
        Self {
            kind,
            resource,
            action: "",
            owner: None,
            run: None,
        }
    }

    #[must_use]
    pub const fn action(mut self, action: &'a str) -> Self {
        self.action = action;
        self
    }

    #[must_use]
    pub const fn owner(mut self, owner: Option<&'a str>) -> Self {
        self.owner = owner;
        self
    }

    #[must_use]
    pub const fn run(mut self, run: Option<&'a str>) -> Self {
        self.run = run;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Coverage {
    Held,
    Stored(AccessGrantRow),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantDraft {
    pub kind: GrantKind,
    pub resource: String,
    pub action: String,
    pub decision: GrantDecision,
    pub owner: Option<String>,
    pub granted_by: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GrantFilter {
    pub kind: Option<GrantKind>,
    pub owner: Option<String>,
}

type Held = HashMap<String, BTreeSet<(GrantKind, String)>>;

struct Inner {
    database: Arc<Database>,
    clock: SharedClock,
    held: Mutex<Held>,
}

#[derive(Clone)]
pub struct AccessGrants(Arc<Inner>);

fn matches(row: &AccessGrantRow, query: &GrantQuery<'_>) -> bool {
    if row.decision != GrantDecision::Allow || row.action != query.action {
        return false;
    }
    match query.kind {
        GrantKind::Folder => folder_covers(Path::new(&row.resource), Path::new(query.resource)),
        _ => row.resource == query.resource,
    }
}

impl AccessGrants {
    pub fn new(database: Arc<Database>, clock: SharedClock) -> Self {
        Self(Arc::new(Inner {
            database,
            clock,
            held: Mutex::new(HashMap::new()),
        }))
    }

    fn held_map(&self) -> MutexGuard<'_, Held> {
        self.0.held.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn covers(&self, query: &GrantQuery<'_>) -> Result<Option<Coverage>, AppError> {
        if let Some(run) = query.run
            && self.held(run, query.kind).iter().any(|resource| {
                matches(
                    &AccessGrantRow {
                        id: String::new(),
                        resource_kind: query.kind,
                        resource: resource.clone(),
                        action: query.action.to_owned(),
                        decision: GrantDecision::Allow,
                        scope: GrantScope::Always,
                        owner: None,
                        granted_by: String::new(),
                        granted_at: 0,
                        used_at: None,
                        detail: None,
                    },
                    query,
                )
            })
        {
            return Ok(Some(Coverage::Held));
        }
        let rows = self.0.database.with_connection(|connection| {
            Ok(Repositories::new(connection)
                .access_grants
                .applicable(query.kind, query.owner.unwrap_or_default())?)
        })?;
        Ok(rows
            .into_iter()
            .find(|row| matches(row, query))
            .map(Coverage::Stored))
    }

    pub fn grant(&self, draft: GrantDraft) -> Result<AccessGrantRow, AppError> {
        let row = AccessGrantRow {
            id: new_id().hyphenated().to_string(),
            resource_kind: draft.kind,
            resource: draft.resource,
            action: draft.action,
            decision: draft.decision,
            scope: GrantScope::of(draft.owner.as_deref()),
            owner: draft.owner,
            granted_by: draft.granted_by,
            granted_at: self.0.clock.now(),
            used_at: None,
            detail: draft.detail,
        };
        self.0.database.transaction(|transaction| {
            Ok(Repositories::new(transaction).access_grants.replace(&row)?)
        })
    }

    pub fn revoke(&self, id: &str) -> Result<bool, AppError> {
        self.0.database.with_connection(|connection| {
            Ok(Repositories::new(connection).access_grants.remove(id)?)
        })
    }

    pub fn list(&self, filter: &GrantFilter) -> Result<Vec<AccessGrantRow>, AppError> {
        let rows = self.0.database.with_connection(|connection| {
            Ok(Repositories::new(connection)
                .access_grants
                .list(filter.kind)?)
        })?;
        Ok(match &filter.owner {
            Some(owner) => rows
                .into_iter()
                .filter(|row| row.owner.as_deref() == Some(owner.as_str()))
                .collect(),
            None => rows,
        })
    }

    pub fn tool_grants(&self) -> Result<Vec<ToolGrantDto>, AppError> {
        let (rows, tools) = self.0.database.with_connection(|connection| {
            let repositories = Repositories::new(connection);
            Ok((
                repositories.access_grants.list(Some(GrantKind::Tool))?,
                repositories.tools.list()?,
            ))
        })?;
        Ok(rows
            .into_iter()
            .map(|row| ToolGrantDto {
                display_name: tools
                    .iter()
                    .find(|tool| tool.key == row.resource)
                    .map_or_else(|| row.resource.clone(), |tool| tool.display_name.clone()),
                tier: match row.decision {
                    GrantDecision::Allow => ToolTier::Auto,
                    GrantDecision::Ask => ToolTier::Ask,
                    GrantDecision::Block => ToolTier::Off,
                },
                id: row.id,
                tool: row.resource,
                scope: row.action,
                granted_by: row.granted_by,
                granted_at: row.granted_at,
            })
            .collect())
    }

    pub fn hold(&self, run: &str, kind: GrantKind, resources: impl IntoIterator<Item = String>) {
        self.held_map()
            .entry(run.to_owned())
            .or_default()
            .extend(resources.into_iter().map(|resource| (kind, resource)));
    }

    pub fn held(&self, run: &str, kind: GrantKind) -> BTreeSet<String> {
        self.held_map()
            .get(run)
            .map(|held| {
                held.iter()
                    .filter(|(held_kind, _)| *held_kind == kind)
                    .map(|(_, resource)| resource.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn release(&self, run: &str) {
        self.held_map().remove(run);
    }

    pub fn uncovered(&self, request: &AccessRequest) -> Result<Vec<Resource>, AppError> {
        let owner = request.prompt.conversation.as_deref();
        let run = Some(request.actor.run_id.as_str()).filter(|run| !run.is_empty());
        let mut pending = Vec::with_capacity(request.resources.len());
        for resource in &request.resources {
            let query = match resource {
                Resource::Folder { path, .. } => GrantQuery::new(GrantKind::Folder, path),
                Resource::App { exe, .. } => GrantQuery::new(GrantKind::App, exe),
                Resource::Site(_) | Resource::Service(_) => {
                    pending.push(resource.clone());
                    continue;
                }
            };
            if self.covers(&query.owner(owner).run(run))?.is_none() {
                pending.push(resource.clone());
            }
        }
        Ok(pending)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::access::Effect;
    use crate::platform::clock::FakeClock;
    use crate::testing::{TemporaryDatabase, temporary_database};

    fn setup() -> (TemporaryDatabase, AccessGrants) {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                connection.execute(
                    "INSERT INTO conversation (id, title, provider_id, model_id, settings, \
                     system_prompt, attached_store_ids, created_at, updated_at) \
                     VALUES ('c1', 'Диалог', 'p', 'model', '{}', '', '[]', 1, 1)",
                    [],
                )?;
                Ok(())
            })
            .expect("seeds");
        let grants = AccessGrants::new(database.shared(), Arc::new(FakeClock::default()));
        (database, grants)
    }

    fn draft(kind: GrantKind, resource: &str, owner: Option<&str>) -> GrantDraft {
        GrantDraft {
            kind,
            resource: resource.to_owned(),
            action: String::new(),
            decision: GrantDecision::Allow,
            owner: owner.map(str::to_owned),
            granted_by: "user".to_owned(),
            detail: None,
        }
    }

    #[test]
    fn a_folder_grant_covers_its_subfolders_for_its_owner_only() {
        let (_database, grants) = setup();
        let row = grants
            .grant(draft(GrantKind::Folder, "C:/work", Some("c1")))
            .expect("grants");
        let query = |path, owner| {
            grants
                .covers(&GrantQuery::new(GrantKind::Folder, path).owner(owner))
                .expect("reads")
        };
        assert_eq!(
            query("C:/Work/notes", Some("c1")),
            Some(Coverage::Stored(row.clone()))
        );
        assert_eq!(query("C:/work", Some("c2")), None);
        assert_eq!(query("C:/other", Some("c1")), None);
        assert!(grants.revoke(&row.id).expect("revokes"));
        assert_eq!(query("C:/work", Some("c1")), None);
    }

    #[test]
    fn only_an_allow_with_the_same_action_covers_a_site() {
        let (_database, grants) = setup();
        let mut blocked = draft(GrantKind::Site, "https://shop.example", None);
        blocked.action = "click".to_owned();
        blocked.decision = GrantDecision::Block;
        grants.grant(blocked.clone()).expect("grants");
        let query = |action| {
            grants
                .covers(&GrantQuery::new(GrantKind::Site, "https://shop.example").action(action))
                .expect("reads")
        };
        assert_eq!(query("click"), None);
        blocked.decision = GrantDecision::Allow;
        grants.grant(blocked).expect("replaces");
        assert!(query("click").is_some());
        assert_eq!(query("type"), None);
        assert_eq!(
            grants
                .list(&GrantFilter {
                    kind: Some(GrantKind::Site),
                    owner: None,
                })
                .expect("lists")
                .iter()
                .filter(|row| row.resource == "https://shop.example")
                .count(),
            1
        );
    }

    #[test]
    fn run_grants_stay_in_memory_until_the_run_is_released() {
        let (_database, grants) = setup();
        grants.hold("r1", GrantKind::App, ["notepad.exe".to_owned()]);
        let query = GrantQuery::new(GrantKind::App, "notepad.exe").run(Some("r1"));
        assert_eq!(grants.covers(&query).expect("reads"), Some(Coverage::Held));
        assert_eq!(grants.covers(&query.run(Some("r2"))).expect("reads"), None);
        assert!(grants.held("r1", GrantKind::Folder).is_empty());
        grants.release("r1");
        assert_eq!(grants.covers(&query).expect("reads"), None);
        assert!(
            grants
                .list(&GrantFilter::default())
                .expect("lists")
                .iter()
                .all(|row| row.resource_kind != GrantKind::App)
        );
    }

    #[test]
    fn the_grants_step_drops_covered_folders_and_apps_from_a_request() {
        let (_database, grants) = setup();
        grants
            .grant(draft(GrantKind::Folder, "C:/work", None))
            .expect("grants");
        grants.hold("r1", GrantKind::App, ["calc.exe".to_owned()]);
        let mut request =
            AccessRequest::new(crate::access::Actor::run("r1"), "fs.apply", Effect::Plain);
        request.prompt.conversation = Some("c1".to_owned());
        request.resources = vec![
            Resource::Folder {
                path: "C:/work/src".to_owned(),
                creates: Vec::new(),
            },
            Resource::Folder {
                path: "C:/elsewhere".to_owned(),
                creates: Vec::new(),
            },
            Resource::App {
                exe: "calc.exe".to_owned(),
                path: "C:/Windows/calc.exe".to_owned(),
                icon: None,
            },
            Resource::App {
                exe: "notepad.exe".to_owned(),
                path: "C:/Windows/notepad.exe".to_owned(),
                icon: None,
            },
        ];
        assert_eq!(
            grants.uncovered(&request).expect("reads"),
            [request.resources[1].clone(), request.resources[3].clone()]
        );
    }

    #[test]
    fn deleting_a_dialog_removes_its_grants() {
        let (database, grants) = setup();
        grants
            .grant(draft(GrantKind::App, "calc.exe", Some("c1")))
            .expect("grants");
        grants
            .grant(draft(GrantKind::Folder, "C:/work", Some("c1")))
            .expect("grants");
        database
            .with_connection(|connection| {
                connection.execute("DELETE FROM conversation WHERE id = 'c1'", [])?;
                Ok(())
            })
            .expect("deletes");
        assert!(
            grants
                .list(&GrantFilter {
                    kind: None,
                    owner: Some("c1".to_owned()),
                })
                .expect("lists")
                .is_empty()
        );
    }
}
