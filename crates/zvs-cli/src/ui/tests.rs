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
    assert!(screen.contains("/models"));
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
