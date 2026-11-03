use super::{text::wrap, theme};
use crate::{
    copy,
    state::{App, modal::Content, transcript::subagent_run},
};
use ratatui::{
    Frame,
    layout::{Margin, Rect},
    text::{Line, Span},
    widgets::{Block, Clear, Padding, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
};
use zvs_contract::AgenticPlanStepStatus;

pub fn content(app: &App, width: usize) -> (&'static str, Vec<Line<'static>>) {
    let help = Content::Help;
    let kind = app.modal.as_ref().map_or(&help, |m| &m.content);
    match kind {
        Content::Help => (copy::HELP_TITLE, super::help_lines(app, width)),
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

fn panel() -> Block<'static> {
    Block::new()
        .padding(Padding::new(2, 2, 1, 1))
        .style(theme::text().bg(theme::colors().panel_bg))
}

pub fn regions(area: Rect) -> (Rect, Rect, Rect) {
    let target = area.inner(Margin::new(2.min(area.width / 4), 1.min(area.height / 4)));
    let inner = panel().inner(target);
    let body = Rect {
        y: inner.y + 2.min(inner.height),
        height: inner.height.saturating_sub(3),
        width: inner.width.saturating_sub(1),
        ..inner
    };
    (target, inner, body)
}

pub fn draw(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let (target, inner, body) = regions(area);
    let (title, lines) = content(app, usize::from(body.width.max(1)));
    let max = lines.len().saturating_sub(usize::from(body.height));
    let offset = app.modal.as_ref().map_or(0, |m| m.offset).min(max);
    frame.render_widget(Clear, target);
    frame.render_widget(panel(), target);
    if inner.height > 0 {
        frame.render_widget(
            Paragraph::new(Span::styled(title, theme::strong())),
            Rect { height: 1, ..inner },
        );
    }
    frame.render_widget(
        Paragraph::new(lines.clone()).scroll((u16::try_from(offset).unwrap_or(u16::MAX), 0)),
        body,
    );
    let mut scroll = ScrollbarState::new(lines.len())
        .position(offset)
        .viewport_content_length(usize::from(body.height));
    frame.render_stateful_widget(
        Scrollbar::new(ScrollbarOrientation::VerticalRight),
        Rect {
            width: inner.width,
            ..body
        },
        &mut scroll,
    );
    if inner.height > 0 {
        frame.render_widget(
            Paragraph::new(Span::styled(copy::MODAL_HINT, theme::faint())),
            Rect {
                y: inner.bottom().saturating_sub(1),
                height: 1,
                ..inner
            },
        );
    }
}
