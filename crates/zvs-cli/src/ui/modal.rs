use super::text::wrap;
use crate::{
    copy,
    state::{App, modal::Content, transcript::subagent_run},
};
use ratatui::text::Line;
use zvs_contract::AgenticPlanStepStatus;

pub fn content(app: &App, width: usize) -> (&'static str, Vec<Line<'static>>) {
    let Some(modal) = &app.modal else {
        return ("", Vec::new());
    };
    match &modal.content {
        Content::Warning(message) => (
            copy::ATTACHMENTS_TITLE,
            wrap(message, width).into_iter().map(Line::from).collect(),
        ),
        Content::GoalPlan => {
            let lines = if app.goal_plan.is_empty() {
                vec![Line::from(copy::GOAL_PLAN_EMPTY)]
            } else {
                app.goal_plan
                    .iter()
                    .flat_map(|step| {
                        let state = match step.status {
                            AgenticPlanStepStatus::Queued => copy::GOAL_STEP_QUEUED,
                            AgenticPlanStepStatus::Running => copy::PROCESS_RUNNING,
                            AgenticPlanStepStatus::Done => copy::GOAL_STEP_DONE,
                            AgenticPlanStepStatus::Failed => copy::PROCESS_FAILED,
                        };
                        let text = format!(
                            "{} · {state}\n{}",
                            step.title,
                            step.summary.as_deref().unwrap_or_default()
                        );
                        text.lines()
                            .flat_map(|line| wrap(line, width))
                            .map(Line::from)
                            .collect::<Vec<_>>()
                    })
                    .collect()
            };
            (copy::GOAL_PLAN_TITLE, lines)
        }
        Content::Subagent(id) => {
            let mut text = vec![id.clone()];
            if let Some(call) = app
                .transcript
                .subagents()
                .iter()
                .find(|c| subagent_run(c) == Some(id.as_str()))
            {
                if let Some(task) = call
                    .arguments
                    .get("task")
                    .and_then(serde_json::Value::as_str)
                {
                    text.push(task.to_owned());
                }
                text.push(crate::state::transcript::subagent_status(call).to_owned());
                if let Some(result) = call.error.as_ref().or(call.result.as_ref()) {
                    text.push(result.clone());
                }
                if let Some(meta) = &call.meta {
                    for key in ["model", "summary", "result", "report"] {
                        if let Some(value) = meta.get(key).and_then(serde_json::Value::as_str) {
                            text.push(value.to_owned());
                        }
                    }
                }
                if let Some(live) = &app.transcript.live {
                    for child in live.children_of(call) {
                        text.push(format!(
                            "{} · {}",
                            child.name,
                            super::transcript::status_look(child.status, app.frame).2
                        ));
                        text.push(
                            serde_json::to_string_pretty(&child.arguments).unwrap_or_default(),
                        );
                        if let Some(result) = child.error.as_ref().or(child.result.as_ref()) {
                            text.push(result.to_string());
                        }
                    }
                }
            } else {
                text.push(copy::SUBAGENT_NOT_FOUND.to_owned());
            }
            (
                copy::SUBAGENT_DETAILS,
                text.iter()
                    .flat_map(|s| s.lines().flat_map(|line| wrap(line, width)))
                    .map(Line::from)
                    .collect(),
            )
        }
    }
}
