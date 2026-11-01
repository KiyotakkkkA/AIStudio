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
    assert!(screen.contains("OpenRouter / GPT-4o mini"));
    assert!(screen.contains(copy::MODE_ALWAYS_ASK));
    assert!(screen.contains(&crate::version::label()));
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
fn a_conversation_renders_user_tool_and_answer_blocks() {
    let screen = render(&conversation_app(), 90, 24);
    println!("{screen}");
    assert!(screen.contains("  Прочитай README"));
    assert!(screen.contains(" ✓ read_file "));
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
}

#[test]
fn help_and_tiny_terminals_do_not_panic() {
    let mut app = conversation_app();
    app.help = true;
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
fn the_shortcut_list_and_the_rebinding_panel_render() {
    let mut app = ready(WorkingFolderAccess::Granted);
    app.update(Msg::Key(Key::Char('?')));
    let screen = render(&app, 120, 40);
    assert!(screen.contains(copy::SHORTCUTS_TITLE), "{screen}");
    for title in [
        copy::KEYS_COMPOSE,
        copy::KEYS_SESSION,
        copy::KEYS_TRANSCRIPT,
    ] {
        assert!(screen.contains(title), "{screen}");
    }
    assert!(screen.contains("Ctrl+O"), "{screen}");
    assert!(screen.contains(copy::KEY_DETAILS), "{screen}");
    assert!(screen.contains(copy::HINT_KEYS), "{screen}");
    let narrow = render(&app, 50, 40);
    assert!(narrow.contains(copy::KEYS_TRANSCRIPT), "{narrow}");

    for value in "/shortcuts".chars() {
        app.update(Msg::Key(Key::Char(value)));
    }
    app.update(Msg::Key(Key::Enter));
    let screen = render(&app, 100, 30);
    assert!(screen.contains(copy::SHORTCUT_COLUMN_KEYS), "{screen}");
    let starts: Vec<usize> = shortcuts::SHORTCUTS
        .iter()
        .map(|shortcut| column_of(&screen, shortcut.description()))
        .collect();
    assert!(
        starts.windows(2).all(|pair| pair[0] == pair[1]),
        "{starts:?}
{screen}"
    );

    app.update(Msg::Key(Key::Enter));
    let capturing = render(&app, 100, 30);
    assert!(capturing.contains(copy::SHORTCUT_PRESS), "{capturing}");
    assert!(capturing.contains(copy::HINT_CAPTURE), "{capturing}");
    assert_eq!(
        column_of(&capturing, shortcuts::SHORTCUTS[0].description()),
        starts[0]
    );
    let _ = render(&app, 20, 6);
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
        "round": 0, "arguments": {}, "status": "succeeded"
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
    assert!(text[2].contains("read_2"));
    assert!(text.last().expect("a line").contains("read_6"));
    assert!(text[2].starts_with("      "));
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
