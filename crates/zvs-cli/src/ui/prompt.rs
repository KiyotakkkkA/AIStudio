use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use zvs_contract::{FileChangeKind, QuestionKind};

use super::text::{truncate, wrap};
use super::theme;
use crate::copy;
use crate::state::prompt::{ApprovalDetail, Prompt, PromptKind, QuestionState};

const CHANGES_SHOWN: usize = 6;

pub struct Panel {
    pub title: String,
    pub lines: Vec<Line<'static>>,
    pub cursor: Option<(u16, u16)>,
}

fn option(label: &str, selected: bool) -> Line<'static> {
    if selected {
        Line::from(vec![
            Span::styled(" › ", theme::selected()),
            Span::styled(format!("{label} "), theme::selected()),
        ])
    } else {
        Line::from(vec![
            Span::raw("   "),
            Span::styled(label.to_owned(), theme::text()),
        ])
    }
}

fn labelled(label: &str, value: &str, width: usize) -> Vec<Line<'static>> {
    let room = width.saturating_sub(label.chars().count() + 4).max(10);
    wrap(value, room)
        .into_iter()
        .enumerate()
        .map(|(index, piece)| {
            let head = if index == 0 {
                format!("{label}: ")
            } else {
                " ".repeat(label.chars().count() + 2)
            };
            Line::from(vec![
                Span::styled(head, theme::muted()),
                Span::styled(piece, theme::accent()),
            ])
        })
        .collect()
}

fn change_mark(kind: FileChangeKind) -> (&'static str, Style) {
    match kind {
        FileChangeKind::Create => ("+", Style::new().fg(theme::OK)),
        FileChangeKind::Modify => ("~", Style::new().fg(theme::WARN)),
        FileChangeKind::Move => ("→", Style::new().fg(theme::WARN)),
        FileChangeKind::Delete => ("−", Style::new().fg(theme::ERR)),
    }
}

fn choices(prompt: &Prompt, lines: &mut Vec<Line<'static>>) {
    lines.push(Line::default());
    let files = prompt.is_files();
    for (index, choice) in prompt.choices().into_iter().enumerate() {
        lines.push(option(choice.label(files), index == prompt.selected));
    }
}

fn question(state: &QuestionState, selected: usize, width: usize) -> Panel {
    let mut lines = Vec::new();
    let mut cursor = None;
    let total = state.request.questions.len();
    let title = format!(
        "{} {} {} {total}",
        copy::QUESTION_TITLE,
        state.index + 1,
        copy::QUESTION_PROGRESS
    );
    let Some(current) = state.current() else {
        return Panel {
            title,
            lines,
            cursor,
        };
    };
    for piece in wrap(&current.title, width.saturating_sub(4)) {
        lines.push(Line::from(Span::styled(piece, theme::strong())));
    }
    lines.push(Line::default());
    let multiple = current.kind == QuestionKind::Multiple;
    for (index, item) in current.options.iter().enumerate() {
        let mark = if multiple {
            if state.checked.get(index).copied().unwrap_or(false) {
                "[x] "
            } else {
                "[ ] "
            }
        } else {
            ""
        };
        let mut line = option(&format!("{mark}{}", item.label), index == selected);
        if item.recommended {
            line.spans.push(Span::styled(
                format!(" · {}", copy::RECOMMENDED),
                theme::faint(),
            ));
        }
        lines.push(line);
        if let Some(description) = &item.description {
            lines.push(Line::from(vec![
                Span::raw("     "),
                Span::styled(
                    truncate(description, width.saturating_sub(8)),
                    theme::faint(),
                ),
            ]));
        }
    }
    if current.allow_other {
        lines.push(option(
            copy::QUESTION_OTHER,
            selected == current.options.len(),
        ));
    }
    if let Some(other) = &state.other {
        lines.push(Line::default());
        let row = u16::try_from(lines.len()).unwrap_or(u16::MAX);
        let (_, column) = other.lines_before_cursor();
        lines.push(Line::from(vec![
            Span::styled(" › ", theme::accent()),
            Span::styled(other.text().replace('\n', " "), theme::text()),
        ]));
        cursor = Some((u16::try_from(column + 3).unwrap_or(u16::MAX), row));
        lines.push(Line::from(Span::styled(
            copy::QUESTION_TEXT_HINT,
            theme::faint(),
        )));
    } else if multiple {
        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            copy::QUESTION_MULTI_HINT,
            theme::faint(),
        )));
    }
    Panel {
        title,
        lines,
        cursor,
    }
}

pub fn verification(origin: &str, width: usize) -> Panel {
    let mut lines: Vec<Line<'static>> = wrap(copy::VERIFY_BODY, width.saturating_sub(4))
        .into_iter()
        .map(|piece| Line::from(Span::styled(piece, theme::text())))
        .collect();
    lines.push(Line::default());
    lines.extend(labelled(copy::VERIFY_SITE, origin, width));
    Panel {
        title: copy::VERIFY_TITLE.to_owned(),
        lines,
        cursor: None,
    }
}

pub fn panel(prompt: &Prompt, width: usize) -> Panel {
    let mut lines = Vec::new();
    let title = match &prompt.kind {
        PromptKind::Access { folder } => {
            for piece in wrap(copy::ACCESS_BODY, width.saturating_sub(4)) {
                lines.push(Line::from(Span::styled(piece, theme::text())));
            }
            lines.push(Line::default());
            lines.extend(labelled(copy::WORKSPACE, folder, width));
            choices(prompt, &mut lines);
            copy::ACCESS_TITLE.to_owned()
        }
        PromptKind::Approval {
            subject, detail, ..
        } => {
            lines.push(Line::from(Span::styled(subject.clone(), theme::strong())));
            match detail {
                ApprovalDetail::Plain => {}
                ApprovalDetail::Process { argv, cwd, .. } => {
                    lines.extend(labelled(copy::COMMAND, &argv.join(" "), width));
                    lines.extend(labelled(copy::IN_FOLDER, cwd, width));
                }
                ApprovalDetail::Files { changes } => {
                    lines.push(Line::from(Span::styled(
                        format!("{}:", copy::CHANGES),
                        theme::muted(),
                    )));
                    for change in changes.iter().take(CHANGES_SHOWN) {
                        let (mark, style) = change_mark(change.kind);
                        lines.push(Line::from(vec![
                            Span::styled(format!(" {mark} "), style.add_modifier(Modifier::BOLD)),
                            Span::styled(
                                truncate(&change.path, width.saturating_sub(20)),
                                theme::text(),
                            ),
                            Span::styled(
                                format!("  +{}", change.added),
                                Style::new().fg(theme::OK),
                            ),
                            Span::styled(
                                format!(" −{}", change.removed),
                                Style::new().fg(theme::ERR),
                            ),
                        ]));
                    }
                    let hidden = changes.len().saturating_sub(CHANGES_SHOWN);
                    if hidden > 0 {
                        lines.push(Line::from(Span::styled(
                            format!(" … {} {hidden}", copy::MORE_LINES),
                            theme::faint(),
                        )));
                    }
                }
            }
            choices(prompt, &mut lines);
            copy::APPROVAL_TITLE.to_owned()
        }
        PromptKind::Folder { folders, .. } => {
            for folder in folders {
                lines.extend(labelled(copy::IN_FOLDER, folder, width));
            }
            choices(prompt, &mut lines);
            copy::FOLDER_REQUEST_TITLE.to_owned()
        }
        PromptKind::Question(state) => return question(state, prompt.selected, width),
    };
    Panel {
        title,
        lines,
        cursor: None,
    }
}
