use std::collections::HashSet;

use serde_json::Value;
use zvs_contract::validate::{js_slice, js_trim};
use zvs_contract::{
    AGENTIC_PLAN_MAX, AGENTIC_STEP_TITLE_MAX, AGENTIC_SUMMARY_MAX, AgenticChangedFileDto,
    AgenticPlanEditDto, AgenticPlanStepDto, AgenticPlanStepStatus, FileDiffDto, StepId,
};

pub const REPEAT_LIMIT: u32 = 3;
pub const REVISION_LIMIT: u32 = 4;

pub type Plan = Vec<AgenticPlanStepDto>;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlanChange {
    pub removed: Vec<String>,
    pub added: Vec<String>,
    pub changed: bool,
}

fn title(text: &str) -> Option<String> {
    let trimmed = js_trim(text);
    (!trimmed.is_empty()).then(|| js_slice(trimmed, AGENTIC_STEP_TITLE_MAX).to_owned())
}

pub fn summary(text: &str) -> Option<String> {
    let trimmed = js_trim(text);
    (!trimmed.is_empty()).then(|| js_slice(trimmed, AGENTIC_SUMMARY_MAX).to_owned())
}

fn next_number(plan: &Plan) -> u64 {
    plan.iter()
        .filter_map(|step| step.id.strip_prefix('p')?.parse::<u64>().ok())
        .max()
        .unwrap_or(0)
        + 1
}

fn fresh(id: String, title: String) -> AgenticPlanStepDto {
    AgenticPlanStepDto {
        id,
        title,
        status: AgenticPlanStepStatus::Queued,
        step_ids: Vec::new(),
        summary: None,
    }
}

const fn settled(step: &AgenticPlanStepDto) -> bool {
    matches!(
        step.status,
        AgenticPlanStepStatus::Done | AgenticPlanStepStatus::Failed
    )
}

pub fn current(plan: &mut Plan) -> Option<&mut AgenticPlanStepDto> {
    let index = plan
        .iter()
        .position(|step| step.status == AgenticPlanStepStatus::Running)
        .or_else(|| {
            plan.iter()
                .position(|step| step.status == AgenticPlanStepStatus::Queued)
        })?;
    let step = &mut plan[index];
    step.status = AgenticPlanStepStatus::Running;
    Some(step)
}

pub fn propose(plan: &mut Plan, titles: &[String]) -> usize {
    let mut number = next_number(plan);
    let (kept, open): (Plan, Plan) = plan.drain(..).partition(settled);
    let mut reusable = open;
    let mut next = kept;
    for text in titles {
        if next.len() >= AGENTIC_PLAN_MAX {
            break;
        }
        let Some(text) = title(text) else {
            continue;
        };
        let step = match reusable.iter().position(|step| step.title == text) {
            Some(index) => {
                let mut step = reusable.remove(index);
                step.status = AgenticPlanStepStatus::Queued;
                step
            }
            None => {
                let step = fresh(format!("p{number}"), text);
                number += 1;
                step
            }
        };
        next.push(step);
    }
    *plan = next;
    current(plan);
    plan.iter().filter(|step| !settled(step)).count()
}

pub fn complete(plan: &mut Plan, text: &str) -> Option<String> {
    let step = plan
        .iter_mut()
        .find(|step| step.status == AgenticPlanStepStatus::Running)?;
    step.status = AgenticPlanStepStatus::Done;
    step.summary = summary(text);
    let done = step.title.clone();
    current(plan);
    Some(done)
}

pub fn finish(plan: &mut Plan) {
    for step in plan.iter_mut() {
        if step.status == AgenticPlanStepStatus::Running {
            step.status = AgenticPlanStepStatus::Done;
        }
    }
}

pub fn fail(plan: &mut Plan) {
    for step in plan.iter_mut() {
        if step.status == AgenticPlanStepStatus::Running {
            step.status = AgenticPlanStepStatus::Failed;
        }
    }
}

pub fn link(plan: &mut Plan, step_id: &str) {
    let Some(id) = StepId::parse(step_id) else {
        return;
    };
    if let Some(step) = current(plan) {
        step.step_ids.push(id);
    }
}

pub fn edit(plan: &mut Plan, edits: &[AgenticPlanEditDto]) -> PlanChange {
    let mut number = next_number(plan);
    let (fixed, queued): (Plan, Plan) = plan
        .drain(..)
        .partition(|step| step.status != AgenticPlanStepStatus::Queued);
    let mut remaining = queued.clone();
    let mut next = fixed;
    let mut change = PlanChange::default();
    for entry in edits {
        if next.len() >= AGENTIC_PLAN_MAX {
            break;
        }
        let Some(text) = title(&entry.title) else {
            continue;
        };
        let known = entry
            .id
            .as_ref()
            .and_then(|id| remaining.iter().position(|step| &step.id == id));
        let step = if let Some(index) = known {
            let mut step = remaining.remove(index);
            if step.title != text {
                step.title = text;
                change.changed = true;
            }
            step
        } else {
            change.added.push(text.clone());
            let step = fresh(format!("p{number}"), text);
            number += 1;
            step
        };
        next.push(step);
    }
    change.removed = remaining.into_iter().map(|step| step.title).collect();
    let before: Vec<&str> = queued.iter().map(|step| step.id.as_str()).collect();
    let after: Vec<&str> = next
        .iter()
        .filter(|step| step.status == AgenticPlanStepStatus::Queued)
        .map(|step| step.id.as_str())
        .collect();
    change.changed |= before != after;
    *plan = next;
    change
}

pub fn describe(plan: &Plan) -> String {
    if plan.is_empty() {
        return "(no plan yet)".to_owned();
    }
    plan.iter()
        .enumerate()
        .map(|(index, step)| format!("{}. [{}] {}", index + 1, step.status.as_str(), step.title))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn record_changes(files: &mut Vec<AgenticChangedFileDto>, output: &Value) {
    let Some(changes) = output.get("changes").and_then(Value::as_array) else {
        return;
    };
    for change in changes {
        let Ok(diff) = serde_json::from_value::<FileDiffDto>(change.clone()) else {
            continue;
        };
        match files.iter_mut().find(|file| file.path == diff.path) {
            Some(file) => {
                file.added += diff.added;
                file.removed += diff.removed;
            }
            None => files.push(AgenticChangedFileDto {
                path: diff.path,
                added: diff.added,
                removed: diff.removed,
            }),
        }
    }
}

#[derive(Debug, Default)]
pub struct LoopGuard {
    last: Option<String>,
    repeats: u32,
    revisions: u32,
    removed: HashSet<String>,
}

impl LoopGuard {
    pub fn round(&mut self, signature: String) -> bool {
        if self.last.as_ref() == Some(&signature) {
            self.repeats += 1;
        } else {
            self.last = Some(signature);
            self.repeats = 1;
        }
        self.repeats >= REPEAT_LIMIT
    }

    pub fn revised(&mut self) -> bool {
        self.revisions += 1;
        self.revisions >= REVISION_LIMIT
    }

    pub fn progressed(&mut self) {
        self.revisions = 0;
    }

    pub fn forbid(&mut self, titles: &[String]) {
        self.removed.extend(titles.iter().cloned());
    }

    pub fn allowed(&self, titles: &[String]) -> Vec<String> {
        titles
            .iter()
            .filter(|text| title(text).is_none_or(|text| !self.removed.contains(&text)))
            .cloned()
            .collect()
    }
}

pub fn signature(calls: &[(String, String)]) -> String {
    calls
        .iter()
        .map(|(name, arguments)| {
            let normalised = serde_json::from_str::<Value>(arguments)
                .map_or_else(|_| arguments.trim().to_owned(), |value| value.to_string());
            format!("{name}:{normalised}")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn titles(plan: &Plan) -> Vec<(&str, &str)> {
        plan.iter()
            .map(|step| (step.title.as_str(), step.status.as_str()))
            .collect()
    }

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| (*item).to_owned()).collect()
    }

    #[test]
    fn a_proposal_starts_the_first_step_and_a_revision_keeps_finished_ones() {
        let mut plan = Plan::new();
        assert_eq!(propose(&mut plan, &strings(&["Прочитать", "Написать"])), 2);
        assert_eq!(
            titles(&plan),
            [("Прочитать", "running"), ("Написать", "queued")]
        );
        assert_eq!(
            complete(&mut plan, "  прочитано  ").as_deref(),
            Some("Прочитать")
        );
        assert_eq!(plan[0].summary.as_deref(), Some("прочитано"));
        let written = plan[1].id.clone();
        propose(&mut plan, &strings(&["Проверить", "Написать", " "]));
        assert_eq!(
            titles(&plan),
            [
                ("Прочитать", "done"),
                ("Проверить", "running"),
                ("Написать", "queued")
            ]
        );
        assert_eq!(plan[2].id, written);
        assert_eq!(plan[1].id, "p3");
    }

    #[test]
    fn user_edits_touch_only_queued_steps_and_report_what_was_removed() {
        let mut plan = Plan::new();
        propose(&mut plan, &strings(&["A", "B", "C", "D"]));
        let change = edit(
            &mut plan,
            &[
                AgenticPlanEditDto {
                    id: Some("p4".to_owned()),
                    title: "D".to_owned(),
                },
                AgenticPlanEditDto {
                    id: Some("p1".to_owned()),
                    title: "A renamed".to_owned(),
                },
                AgenticPlanEditDto {
                    id: None,
                    title: "E".to_owned(),
                },
                AgenticPlanEditDto {
                    id: Some("p2".to_owned()),
                    title: "B".to_owned(),
                },
            ],
        );
        assert_eq!(
            titles(&plan),
            [
                ("A", "running"),
                ("D", "queued"),
                ("A renamed", "queued"),
                ("E", "queued"),
                ("B", "queued")
            ]
        );
        assert_eq!(change.removed, ["C"]);
        assert_eq!(change.added, ["A renamed", "E"]);
        assert!(change.changed);
        assert_eq!(
            edit(&mut plan.clone(), &[]).removed,
            ["D", "A renamed", "E", "B"]
        );
    }

    #[test]
    fn an_unchanged_edit_is_not_a_change() {
        let mut plan = Plan::new();
        propose(&mut plan, &strings(&["A", "B"]));
        let change = edit(
            &mut plan,
            &[AgenticPlanEditDto {
                id: Some("p2".to_owned()),
                title: "B".to_owned(),
            }],
        );
        assert_eq!(change, PlanChange::default());
    }

    #[test]
    fn tool_steps_link_to_the_running_step() {
        let mut plan = Plan::new();
        propose(&mut plan, &strings(&["A"]));
        link(&mut plan, "0199aa11-6666-7111-8111-111111111111");
        link(&mut plan, "not a step");
        assert_eq!(plan[0].step_ids.len(), 1);
        fail(&mut plan);
        assert_eq!(plan[0].status, AgenticPlanStepStatus::Failed);
    }

    #[test]
    fn changed_files_add_up_per_path() {
        let mut files = Vec::new();
        let diff = |path: &str, added: u64, removed: u64| json!({ "path": path, "kind": "modify", "added": added, "removed": removed, "diff": "", "truncated": false });
        record_changes(
            &mut files,
            &json!({ "changes": [diff("C:\\w\\a.md", 3, 1), diff("C:\\w\\b.md", 2, 0)] }),
        );
        record_changes(
            &mut files,
            &json!({ "changes": [diff("C:\\w\\a.md", 1, 1)] }),
        );
        record_changes(&mut files, &json!({ "text": "no changes" }));
        assert_eq!(
            files
                .iter()
                .map(|file| (file.path.as_str(), file.added, file.removed))
                .collect::<Vec<_>>(),
            [("C:\\w\\a.md", 4, 2), ("C:\\w\\b.md", 2, 0)]
        );
    }

    #[test]
    fn the_guard_trips_on_repeated_rounds_and_endless_revisions() {
        let mut guard = LoopGuard::default();
        let same = signature(&[("fs__read".to_owned(), r#"{ "path": "a" }"#.to_owned())]);
        let spaced = signature(&[("fs__read".to_owned(), r#"{"path":"a"}"#.to_owned())]);
        assert_eq!(same, spaced);
        assert!(!guard.round(same.clone()));
        assert!(!guard.round(same.clone()));
        assert!(guard.round(same));
        let mut guard = LoopGuard::default();
        assert!(!guard.revised());
        assert!(!guard.revised());
        guard.progressed();
        for _ in 1..REVISION_LIMIT {
            assert!(!guard.revised());
        }
        assert!(guard.revised());
        guard.forbid(&strings(&["Удалить тесты"]));
        assert_eq!(
            guard.allowed(&strings(&[" Удалить тесты ", "Написать"])),
            ["Написать"]
        );
    }
}
