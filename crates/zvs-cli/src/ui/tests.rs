use ratatui::Terminal;
use ratatui::backend::TestBackend;
use serde_json::{Value, json};
use zvs_contract::{CliSessionDto, ProviderDto, WorkingFolderAccess};

use super::*;
use crate::state::shortcuts;
use crate::state::{Key, Msg};

const PROVIDERS: &str =
    include_str!("../../../../packages/shared/fixtures/providers/ProviderDto.valid.json");

fn ready(access: WorkingFolderAccess) -> App {
    let mut app = App::new(r"D:\work\project");
    app.update(Msg::Opened(Ok(CliSessionDto {
        folder: r"D:\work\project".to_owned(),
        access,
    })));
    let cases: Vec<Value> = serde_json::from_str(PROVIDERS).expect("fixture");
    let provider: ProviderDto =
        serde_json::from_value(cases[0]["value"].clone()).expect("provider");
    app.update(Msg::Providers(Ok(vec![provider])));
    app
}

fn render(app: &App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
    terminal.draw(|frame| draw(frame, app)).expect("draws");
    let buffer = terminal.backend().buffer().clone();
    (0..height)
        .map(|row| {
            (0..width)
                .map(|column| buffer[(column, row)].symbol().to_owned())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn conversation_app() -> App {
    let mut app = ready(WorkingFolderAccess::Granted);
    app.transcript
        .push_user("Прочитай README и кратко перескажи");
    app.transcript
        .entries
        .push(crate::state::transcript::Entry::Tool(Box::new(
            serde_json::from_value(json!({
                "id": "c1", "key": "files.read", "name": "read_file", "round": 0,
                "arguments": { "path": "README.md" }, "status": "succeeded",
                "result": "# ZVS AI Studio\nЛокальная студия", "durationMs": 84
            }))
            .expect("call"),
        )));
    app.transcript
        .entries
        .push(crate::state::transcript::Entry::Assistant {
            text: "## Кратко\n- Локальная студия\n- Запуск через `pnpm dev`".to_owned(),
            reasoning: String::new(),
        });
    app
}

#[test]
fn the_welcome_screen_shows_the_art_folder_model_and_version() {
    let screen = render(&ready(WorkingFolderAccess::Granted), 100, 30);
    println!("{screen}");
    assert!(screen.contains("█████"));
    assert!(screen.contains(r"D:\work\project"));
    assert!(screen.contains("GPT-4o mini"));
    assert!(screen.contains(copy::MODE_ALWAYS_ASK));
    assert!(screen.contains(&crate::version::label()));
}

#[test]
fn the_welcome_banner_is_centered_above_the_composer() {
    let app = ready(WorkingFolderAccess::Granted);
    let [body, ..] = chat_areas(&app, Rect::new(0, 0, 100, 40), 0, false);
    let screen = render(&app, 100, 40);
    let top = screen
        .lines()
        .position(|line| line.contains("█████"))
        .unwrap();
    let title = screen
        .lines()
        .position(|line| line.contains(copy::TITLE))
        .unwrap();
    assert!(top.abs_diff(usize::from(body.bottom()) - title - 1) <= 1);
}

#[test]
fn the_access_prompt_lists_its_three_answers() {
    let screen = render(&ready(WorkingFolderAccess::Missing), 100, 30);
    println!("{screen}");
    assert!(screen.contains(copy::ACCESS_TITLE));
    assert!(screen.contains(copy::ACCESS_CONVERSATION));
    assert!(screen.contains(copy::ACCESS_DENY));
}

#[test]
fn the_access_banner_and_prompt_share_a_centered_group() {
    let screen = render(&ready(WorkingFolderAccess::Missing), 100, 42);
    let top = screen.lines().position(|line| !line.is_empty()).unwrap();
    let bottom = screen
        .lines()
        .enumerate()
        .filter_map(|(row, line)| (!line.is_empty()).then_some(row))
        .last()
        .unwrap();
    let prompt = screen
        .lines()
        .position(|line| line.contains(copy::ACCESS_TITLE))
        .unwrap();
    assert!(top >= 8);
    assert_eq!(prompt - top, 8);
    assert!(top.abs_diff(41 - bottom) <= 1);
}

#[test]
fn a_conversation_renders_user_tool_and_answer_blocks() {
    let screen = render(&conversation_app(), 90, 24);
    println!("{screen}");
    assert!(screen.contains("› Прочитай README"));
    assert!(screen.contains(&format!("✓ {} README.md", copy::TOOL_READ_ACTION)));
    assert!(screen.contains("• Запуск через pnpm dev"));
}

#[test]
fn the_palette_opens_above_the_input() {
    let mut app = ready(WorkingFolderAccess::Granted);
    app.update(Msg::Key(Key::Char('/')));
    let screen = render(&app, 90, 24);
    println!("{screen}");
    assert!(screen.contains(copy::COMMANDS_TITLE));
    assert!(screen.contains("/clear"));
    assert!(!screen.contains(copy::INPUT_PLACEHOLDER));
    assert!(screen.contains("OpenRouter"));
    assert!(screen.contains(r"D:\work\project"));
    assert!(screen.lines().any(|line| line.starts_with("│› /")));
    let area = Rect::new(0, 0, 90, 24);
    let sheet = super::active_sheet(&app, area).expect("command palette");
    let [_, _, _, panel, composer, _] = super::chat_areas(&app, area, sheet.height, sheet.floating);
    assert!(panel.height < 18);
    assert_eq!(panel.bottom(), composer.y);
}

#[test]
fn wheel_scrolls_tool_history_without_moving_the_composer_or_footer() {
    let mut app = conversation_app();
    app.viewport = (100, 20);
    app.transcript
        .entries
        .push(crate::state::transcript::Entry::Assistant {
            text: "A line of the completed answer.\n".repeat(30),
            reasoning: String::new(),
        });
    let tail = render(&app, 100, 20);
    app.update(Msg::Scroll(100, 5, 3));
    let start = render(&app, 100, 20);
    assert!(start.contains(copy::TOOL_READ_ACTION), "{start}");
    assert_eq!(
        start.lines().skip(16).collect::<Vec<_>>(),
        tail.lines().skip(16).collect::<Vec<_>>()
    );
    let position = app.scroll;
    app.update(Msg::Scroll(-3, 5, 18));
    assert_eq!(app.scroll, position);
}

#[test]
fn the_sheet_uses_at_least_three_quarters_and_hides_the_composer_and_status() {
    let mut app = conversation_app();
    app.overlay = Some(crate::state::overlay::Overlay::new(Panel::Theme));
    let area = Rect::new(10, 20, 100, 50);
    let sheet = super::active_sheet(&app, area).expect("theme panel");
    let [history, _, _, panel, composer, status] =
        super::chat_areas(&app, area, sheet.height, sheet.floating);
    assert_eq!(panel.height, 38);
    assert!(history.bottom() <= panel.y);
    assert_eq!(panel.bottom(), area.bottom());
    assert_eq!(composer.height, 0);
    assert_eq!(status.height, 0);
}

#[test]
fn an_approval_prompt_is_visible_when_an_overlay_is_open() {
    let mut app = conversation_app();
    app.overlay = Some(crate::state::overlay::Overlay::new(Panel::Theme));
    let request = json!({
        "id": "0199aa11-7777-7111-8111-111111111111",
        "runId": "0199aa11-8888-7111-8111-111111111111",
        "subject": "demo.tool",
    });
    let prompt = crate::state::prompt::from_request(
        request.as_object().expect("request"),
        zvs_contract::ExecMode::AlwaysAsk,
    )
    .expect("approval");
    app.prompts.push_back(prompt);
    let screen = render(&app, 80, 24);
    assert!(screen.contains(copy::APPROVAL_TITLE), "{screen}");
    assert!(screen.contains(copy::HINT_PROMPT), "{screen}");
}

#[test]
fn panel_search_uses_a_full_width_field_above_its_results() {
    let mut app = conversation_app();
    let mut overlay = crate::state::overlay::Overlay::new(Panel::Shortcuts);
    overlay.tab = 1;
    app.overlay = Some(overlay);
    for width in [70_u16, 110] {
        let screen = render(&app, width, 28);
        let lines: Vec<_> = screen.lines().collect();
        let search = lines
            .iter()
            .position(|line| line.contains(copy::SEARCH_PLACEHOLDER))
            .expect("search field");
        let top = lines[search - 1].trim_start();
        assert!(top.starts_with('╭') && top.ends_with('╮'), "{screen}");
        assert_eq!(top.chars().count(), usize::from(width - 4));
        assert!(lines[search + 1].trim_start().starts_with('╰'));
        assert!(lines[search + 2].contains(copy::SHORTCUT_COLUMN_KEYS));
    }
    for value in "Ctrl+O".chars() {
        app.update(Msg::Key(Key::Char(value)));
    }
    let filtered = render(&app, 110, 28);
    assert!(filtered.lines().any(|line| line.contains("⌕ Ctrl+O")));
    assert!(filtered.contains(shortcuts::SHORTCUTS[0].description()));
}

#[test]
fn help_and_tiny_terminals_do_not_panic() {
    let mut app = conversation_app();
    app.update(Msg::Key(Key::Char('?')));
    for (width, height) in [(90, 30), (20, 6), (1, 1), (200, 60)] {
        render(&app, width, height);
    }
    render(&App::new("D:/x"), 10, 3);
}

fn column_of(screen: &str, text: &str) -> usize {
    let line = screen
        .lines()
        .find(|line| line.contains(text))
        .unwrap_or_else(|| {
            panic!(
                "a row for {text}
{screen}"
            )
        });
    line[..line.find(text).expect("found")].chars().count()
}

#[test]
fn skill_columns_share_origins_and_expand_with_the_terminal() {
    let mut app = conversation_app();
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "../../../../packages/shared/fixtures/skills/SkillDto.valid.json"
    ))
    .expect("skill fixtures");
    let mut overlay = crate::state::overlay::Overlay::new(Panel::Skills);
    overlay.loading = false;
    overlay.skills = cases
        .iter()
        .map(|case| serde_json::from_value(case["value"].clone()).expect("skill"))
        .collect();
    overlay.skills[1].name = "XML".to_owned();
    overlay.skills[1].file_count = 22;
    app.overlay = Some(overlay);

    for width in [110, 200] {
        let screen = render(&app, width, 35);
        let status = column_of(&screen, copy::LIST_STATUS);
        assert_eq!(column_of(&screen, copy::SKILL_ON), status);
        assert_eq!(column_of(&screen, copy::SKILL_INVALID), status);
        let files = column_of(&screen, copy::LIST_FILES);
        assert_eq!(column_of(&screen, &crate::format::files(1)), files);
        assert_eq!(column_of(&screen, &crate::format::files(22)), files);
        assert!(screen.contains(copy::LIST_DESCRIPTION), "{screen}");
    }
    let wide = render(&app, 200, 35);
    let description = app.overlay.as_ref().expect("skills").skills[0]
        .description
        .as_str();
    assert_eq!(
        column_of(&wide, description),
        column_of(&wide, copy::LIST_DESCRIPTION)
    );
    let narrow = render(&app, 40, 25);
    assert_eq!(
        column_of(&narrow, copy::SKILL_ON),
        column_of(&narrow, copy::LIST_STATUS)
    );
}

#[test]
fn one_reference_renders_both_tabs_and_rebinding() {
    let mut app = ready(WorkingFolderAccess::Granted);
    app.update(Msg::Key(Key::Char('?')));
    let sheet = super::active_sheet(&app, Rect::new(0, 0, 120, 40)).expect("reference");
    assert_eq!(sheet.height, super::rows(super::REFERENCE_ROWS + 4));
    let screen = render(&app, 120, 40);
    assert!(screen.contains(copy::HELP_TITLE), "{screen}");
    assert!(screen.contains(copy::SHORTCUT_COLUMN_KEYS), "{screen}");
    assert!(screen.contains("Ctrl+O"), "{screen}");
    assert!(screen.contains(copy::HINT_REFERENCE_KEYS), "{screen}");
    assert!(!screen.contains(copy::INPUT_PLACEHOLDER), "{screen}");
    assert!(screen.contains(copy::SEARCH_PLACEHOLDER), "{screen}");

    app.update(Msg::Key(Key::Tab));
    let commands = render(&app, 100, 40);
    assert!(commands.contains("/clear"), "{commands}");
    assert!(commands.contains(copy::HINT_REFERENCE_COMMANDS));

    app.update(Msg::Key(Key::Tab));
    let screen = render(&app, 100, 40);
    let starts: Vec<usize> = shortcuts::SHORTCUTS
        .iter()
        .take(4)
        .map(|shortcut| column_of(&screen, shortcut.description()))
        .collect();
    assert!(starts.windows(2).all(|pair| pair[0] == pair[1]), "{screen}");
    app.update(Msg::Key(Key::Enter));
    let capturing = render(&app, 100, 40);
    assert!(capturing.contains(copy::SHORTCUT_PRESS), "{capturing}");
    assert!(capturing.contains(copy::HINT_CAPTURE), "{capturing}");
    assert_eq!(
        column_of(&capturing, shortcuts::SHORTCUTS[0].description()),
        starts[0]
    );
    let narrow = render(&app, 20, 6);
    assert!(narrow.contains("?") || narrow.lines().count() == 6);
}

#[test]
fn the_theme_panel_lists_every_theme_and_redraws_in_the_previewed_one() {
    let mut app = ready(WorkingFolderAccess::Granted);
    for value in "/theme".chars() {
        app.update(Msg::Key(Key::Char(value)));
    }
    app.update(Msg::Key(Key::Enter));
    app.update(Msg::Key(Key::Down));
    let screen = render(&app, 100, 20);
    assert!(screen.contains(copy::THEME_TITLE), "{screen}");
    assert!(screen.contains(copy::HINT_THEME), "{screen}");
    for label in [copy::THEME_EMBER, copy::THEME_GRAPHITE, copy::THEME_FROST] {
        assert!(screen.contains(label), "{screen}");
    }
    assert_eq!(theme::active(), zvs_contract::CliTheme::Graphite);
    app.update(Msg::Key(Key::Esc));
    let _ = render(&app, 100, 20);
    assert_eq!(theme::active(), zvs_contract::CliTheme::Ember);
}

const SERVER_DETAILS: &str =
    include_str!("../../../../packages/shared/fixtures/mcp/McpServerDetailDto.valid.json");

fn inspected_app() -> App {
    let mut app = ready(WorkingFolderAccess::Granted);
    let cases: Vec<Value> = serde_json::from_str(SERVER_DETAILS).expect("fixture");
    app.update(Msg::Inspected(Ok(serde_json::from_value(
        cases[0]["value"].clone(),
    )
    .expect("detail"))));
    app
}

#[test]
fn an_inspected_server_lists_its_metadata_and_folds_its_tools() {
    let mut app = inspected_app();
    let screen = render(&app, 110, 40);
    assert!(screen.contains("⧉ qdrant"));
    assert!(screen.contains("npx -y mcp-server-qdrant"));
    assert!(screen.contains(&format!("QDRANT_API_KEY ({})", copy::MCP_SECRET)));
    assert!(!screen.contains(" kb"));
    assert!(screen.contains(&format!("{} · 1", copy::MCP_TOOLS)));
    assert!(screen.contains(&format!("Ctrl+O — {}", copy::MCP_EXPAND)));
    assert!(!screen.contains("• search"));

    app.update(Msg::Key(Key::Chord(shortcuts::Chord::ctrl('o'))));
    let screen = render(&app, 110, 40);
    assert!(screen.contains("• search"));
    assert!(screen.contains(&format!("[{}]", copy::MCP_READ_ONLY)));
    assert!(screen.contains("• collections"));
    assert!(screen.contains(&format!("Ctrl+O — {}", copy::MCP_COLLAPSE)));
}

#[test]
fn the_palette_shows_subcommands_only_once_their_command_is_typed() {
    let mut app = ready(WorkingFolderAccess::Granted);
    for value in "/mcp".chars() {
        app.update(Msg::Key(Key::Char(value)));
    }
    let screen = render(&app, 110, 24);
    assert!(!screen.contains("/mcp add"));
    app.update(Msg::Key(Key::Char(' ')));
    let screen = render(&app, 110, 24);
    assert!(screen.contains("/mcp add"));
    assert!(screen.contains(copy::CMD_MCP_INSPECT));
}

#[test]
fn the_footer_counts_running_background_processes() {
    let mut app = ready(WorkingFolderAccess::Granted);
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "../../../../packages/shared/fixtures/chat/ConversationDto.valid.json"
    ))
    .expect("fixture");
    let conversation: zvs_contract::ConversationDto =
        serde_json::from_value(cases[0]["value"].clone()).expect("a conversation");
    let owner = conversation.id;
    app.conversation = Some(conversation);
    let shell: zvs_contract::BackgroundShellDto = serde_json::from_value(json!({
        "id": "0199aa11-4444-7111-8111-111111111111",
        "ownerId": owner.to_string(),
        "program": "npx",
        "argv": ["npx", "http-server"],
        "cwd": "C:/work",
        "status": "running",
        "exitCode": null,
        "streamId": "0199aa11-5555-7111-8111-111111111111",
        "startedAt": 1_000,
        "endedAt": null,
    }))
    .expect("a shell");
    app.update(Msg::Processes(owner, Ok(vec![shell])));
    let screen = render(&app, 140, 30);
    assert!(screen.contains(&crate::format::processes(1)));
}

#[test]
fn process_details_list_status_runtime_command_and_output() {
    let mut app = ready(WorkingFolderAccess::Granted);
    let shell: zvs_contract::BackgroundShellDto = serde_json::from_value(json!({
        "id": "0199aa11-4444-7111-8111-111111111111",
        "ownerId": "0199aa11-2222-7111-8111-111111111111",
        "program": "cargo",
        "argv": ["cargo", "test", "--workspace"],
        "cwd": "C:/work",
        "status": "exited",
        "exitCode": 101,
        "streamId": "0199aa11-5555-7111-8111-111111111111",
        "startedAt": 1_000,
        "endedAt": 635_000,
    }))
    .expect("a shell");
    app.processes = vec![shell.clone()];
    let mut overlay =
        crate::state::overlay::Overlay::new(crate::state::overlay::Panel::ProcessOutput);
    let mut output = crate::state::overlay::ProcessOutput::new(&shell);
    output.absorb(
        "running 4 tests
",
        16,
        false,
    );
    overlay.output = Some(output);
    overlay.loading = false;
    app.overlay = Some(overlay);
    let screen = render(&app, 110, 30);
    for expected in [
        copy::PROCESS_OUTPUT_TITLE,
        copy::PROCESS_STATUS,
        "10 мин 34 с",
        "cargo test --workspace",
        copy::PROCESS_EXIT_CODE,
        "101",
        "running 4 tests",
        copy::HINT_PROCESS_OUTPUT,
    ] {
        assert!(screen.contains(expected), "{expected}");
    }
    assert_eq!(screen.matches(copy::HINT_PROCESS_OUTPUT).count(), 1);
}

#[test]
fn the_footer_shows_goal_progress_or_its_pause() {
    let mut app = conversation_app();
    app.goal = Some(crate::state::GoalState {
        usage: zvs_contract::GoalUsageDto {
            rounds: 12,
            tokens: 1000,
            elapsed_ms: 180_000,
        },
        ..crate::state::GoalState::started(0)
    });
    let screen = render(&app, 140, 30);
    println!("{screen}");
    assert!(screen.contains(&crate::format::goal_progress(12, 40, 180_000)));
    app.goal = Some(crate::state::GoalState {
        status: zvs_contract::GoalStatus::Paused,
        reason: Some(zvs_contract::GoalReason::Minutes),
        ..crate::state::GoalState::started(0)
    });
    let screen = render(&app, 140, 30);
    assert!(screen.contains(&crate::format::with_reason(
        copy::GOAL_PAUSED,
        Some(zvs_contract::GoalReason::Minutes)
    )));
}

#[test]
fn a_completed_goal_shows_its_summary_and_three_lines_of_evidence() {
    let mut app = conversation_app();
    app.transcript
        .entries
        .push(crate::state::transcript::Entry::Tool(Box::new(
            serde_json::from_value(json!({
                "id": "g1", "key": "goal.complete", "name": "complete_goal", "round": 3,
                "arguments": {
                    "summary": "Тесты добавлены",
                    "evidence": "первая\nвторая\nтретья\nчетвёртая\nпятая"
                },
                "status": "succeeded", "result": "The goal is recorded as achieved."
            }))
            .expect("call"),
        )));
    let screen = render(&app, 100, 40);
    println!("{screen}");
    assert!(screen.contains(copy::GOAL_DONE));
    assert!(screen.contains("Тесты добавлены"));
    assert!(screen.contains(copy::GOAL_EVIDENCE));
    assert!(screen.contains("третья"));
    assert!(!screen.contains("четвёртая"));
    assert!(screen.contains(&format!("… {} 2", copy::MORE_LINES)));
    assert!(!screen.contains("recorded as achieved"));
    app.expanded = true;
    let screen = render(&app, 100, 40);
    assert!(screen.contains("пятая"));
}

fn plain(lines: &[ratatui::text::Line<'_>]) -> Vec<String> {
    lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect()
        })
        .collect()
}

fn delegate(status: &str, report: &str) -> zvs_contract::ChatToolCallDto {
    serde_json::from_value(json!({
        "id": "d1", "key": "agent.delegate", "name": "delegate_task", "round": 0,
        "arguments": { "task": "Найди x", "preset": "explore" },
        "status": if status == "running" { "running" } else { "succeeded" },
        "meta": { "runId": "0199aa11-8888-7111-8111-111111111111", "preset": "explore",
            "label": "Найди x", "task": "Найди x", "model": "qwen3:8b",
            "modelSource": "preferred", "status": status, "partial": status == "partial",
            "toolCalls": 3, "tokens": 12345, "durationMs": 4200, "report": report }
    }))
    .expect("a delegate call")
}

fn child(index: usize) -> zvs_contract::ChatToolCallDto {
    serde_json::from_value(json!({
        "id": format!("c{index}"), "key": "fs.read", "name": format!("read_{index}"),
        "round": 0, "arguments": {"path": format!("file_{index}.rs")}, "status": "succeeded"
    }))
    .expect("a child call")
}

#[test]
fn a_running_subagent_block_shows_the_header_and_the_last_five_child_calls() {
    let children: Vec<_> = (0..7).map(child).collect();
    let text = plain(&transcript::subagent_block(
        &delegate("running", ""),
        &children,
        false,
        0,
        100,
    ));
    assert!(text[0].contains(copy::SUBAGENT_EXPLORE));
    assert!(text[0].contains("Найди x"));
    assert!(text[0].contains("qwen3:8b"));
    assert!(text[1].contains(copy::SUBAGENT_RUNNING));
    assert!(text[1].contains(&crate::format::tools(7)));
    assert_eq!(text.len(), 2 + transcript::SUBAGENT_STEPS_SHOWN);
    assert!(text[2].contains("file_2.rs"));
    assert!(text.last().expect("a line").contains("file_6.rs"));
    assert!(text[2].starts_with("    ✓ "));
}

#[test]
fn a_finished_subagent_block_collapses_the_report_to_eight_lines() {
    let report = (1..=12)
        .map(|line| format!("строка {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let collapsed = plain(&transcript::subagent_block(
        &delegate("partial", &report),
        &[],
        false,
        0,
        100,
    ));
    assert!(collapsed[1].contains(copy::SUBAGENT_PARTIAL));
    assert!(
        collapsed
            .iter()
            .any(|line| line.contains(copy::SUBAGENT_REPORT))
    );
    assert!(collapsed.iter().any(|line| line.contains("строка 8")));
    assert!(!collapsed.iter().any(|line| line.contains("строка 9")));
    assert!(
        collapsed
            .iter()
            .any(|line| line.contains(&format!("{} 4", copy::MORE_LINES)))
    );
    let footer = collapsed.last().expect("a footer");
    assert!(footer.contains(&crate::format::tokens(12_345)));
    let expanded = plain(&transcript::subagent_block(
        &delegate("ok", &report),
        &[],
        true,
        0,
        100,
    ));
    assert!(expanded.iter().any(|line| line.contains("строка 12")));
}

#[test]
fn a_subagent_prompt_starts_with_its_title_line() {
    let mut request = serde_json::Map::new();
    request.insert(
        "id".to_owned(),
        json!("0199aa11-7777-7111-8111-111111111111"),
    );
    request.insert(
        "runId".to_owned(),
        json!("0199aa11-8888-7111-8111-111111111111"),
    );
    request.insert("subject".to_owned(), json!("demo.drop"));
    request.insert(
        "subagent".to_owned(),
        json!({ "runId": "0199aa11-8888-7111-8111-111111111111", "label": "Удали файлы" }),
    );
    let prompt = crate::state::prompt::from_request(&request, zvs_contract::ExecMode::AlwaysAsk)
        .expect("a prompt");
    let panel = super::prompt::panel(&prompt, 80);
    assert_eq!(plain(&panel.lines)[0], copy::subagent_title("Удали файлы"));
    assert_eq!(panel.title, copy::APPROVAL_TITLE);
}

#[test]
fn the_queue_is_a_muted_section_above_the_input_with_its_shortcuts() {
    let mut app = conversation_app();
    app.queue = vec![
        zvs_contract::ChatQueuedMessageDto {
            id: "q1".to_owned(),
            text: "Добавь тесты".to_owned(),
            created_at: 1,
        },
        zvs_contract::ChatQueuedMessageDto {
            id: "q2".to_owned(),
            text: "И проверь clippy".to_owned(),
            created_at: 2,
        },
    ];
    app.queue_selected = Some(0);
    app.queue_urgent = true;
    let screen = render(&app, 120, 30);
    assert!(screen.contains(&copy::queued(2)), "{screen}");
    assert!(screen.contains(copy::QUEUE_SENDING), "{screen}");
    assert!(screen.contains(" › Добавь тесты"), "{screen}");
    assert!(screen.contains("   И проверь clippy"), "{screen}");
    assert!(screen.contains("Ctrl+↑/Ctrl+↓"), "{screen}");
    assert!(
        screen.contains(&format!("Ctrl+Enter {}", copy::QUEUE_SEND)),
        "{screen}"
    );
    let queue_row = screen
        .lines()
        .position(|line| line.contains("И проверь clippy"))
        .expect("a queued row");
    let input_row = screen
        .lines()
        .position(|line| line.contains("Сообщение…"))
        .unwrap_or(usize::MAX);
    assert!(queue_row < input_row, "{screen}");
    app.queue.clear();
    assert!(!render(&app, 120, 30).contains(copy::QUEUE_TITLE));
}

#[test]
fn a_held_change_is_shown_as_waiting_for_the_app() {
    let mut app = ready(WorkingFolderAccess::Granted);
    app.update(Msg::Notice(zvs_contract::CliNotice::Consent {
        pending: true,
    }));
    let screen = render(&app, 120, 30);
    assert!(screen.contains(copy::CONSENT_PENDING), "{screen}");
    app.update(Msg::Notice(zvs_contract::CliNotice::Consent {
        pending: false,
    }));
    let screen = render(&app, 120, 30);
    assert!(!screen.contains(copy::CONSENT_PENDING), "{screen}");
}
