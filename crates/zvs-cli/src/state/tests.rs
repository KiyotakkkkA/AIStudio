use serde_json::{Value, json};

use super::transcript::Entry;
use super::*;

const PROVIDERS: &str =
    include_str!("../../../../packages/shared/fixtures/providers/ProviderDto.valid.json");

#[test]
fn copilot_login_can_be_cancelled_and_adopts_its_models_after_completion() {
    let mut app = ready();
    typed(&mut app, "/copilot");
    assert_eq!(app.update(Msg::Key(Key::Enter)), vec![Request::LinkCopilot]);
    assert!(app.copilot_linking);
    assert_eq!(app.status.as_ref().unwrap().1, copy::COPILOT_LINKING);
    assert_eq!(
        app.update(Msg::Key(Key::Esc)),
        vec![Request::CancelCopilotLink]
    );
    let mut copilot = provider();
    copilot.summary.adapter = zvs_contract::AdapterFamily::Copilot;
    app.update(Msg::CopilotLinked(Ok(vec![copilot.clone()])));
    assert!(!app.copilot_linking);
    assert_eq!(app.model.as_ref().unwrap().provider, copilot.summary.id);
    assert_eq!(app.effort, ThinkingEffort::Auto);
    assert_eq!(app.status.as_ref().unwrap().1, copy::COPILOT_LINKED);
}
const CONVERSATIONS: &str =
    include_str!("../../../../packages/shared/fixtures/chat/ConversationDto.valid.json");
const RUN: &str = "0199aa11-5555-7111-8111-111111111111";
const STREAM: &str = "0199aa11-6666-7111-8111-111111111111";
const APPROVAL: &str = "0199aa11-7777-7111-8111-111111111111";
const FOLDER: &str = r"D:\work";

fn first<T: serde::de::DeserializeOwned>(fixture: &str) -> T {
    let cases: Vec<Value> = serde_json::from_str(fixture).expect("fixture");
    serde_json::from_value(cases[0]["value"].clone()).expect("parses")
}

fn provider() -> ProviderDto {
    first(PROVIDERS)
}

fn conversation() -> ConversationDto {
    first(CONVERSATIONS)
}

fn handle() -> RunHandleDto {
    serde_json::from_value(json!({ "id": RUN, "streamId": STREAM })).expect("handle")
}

fn event(seq: u64, body: Value) -> Msg {
    let mut value = json!({ "streamId": STREAM, "seq": seq, "ts": 1 });
    if let (Value::Object(target), Value::Object(fields)) = (&mut value, body) {
        target.extend(fields);
    }
    Msg::Event(Box::new(serde_json::from_value(value).expect("event")))
}

fn end(status: &str) -> Msg {
    event(
        99,
        json!({ "type": "end", "outcome": { "status": status } }),
    )
}

fn typed(app: &mut App, text: &str) {
    for value in text.chars() {
        app.update(Msg::Key(Key::Char(value)));
    }
}

fn ready() -> App {
    let mut app = App::new(FOLDER);
    app.update(Msg::Opened(Ok(CliSessionDto {
        folder: FOLDER.to_owned(),
        access: WorkingFolderAccess::Granted,
    })));
    app.update(Msg::Providers(Ok(vec![provider()])));
    app
}

fn running() -> App {
    let mut app = ready();
    app.conversation = Some(conversation());
    typed(&mut app, "привет");
    app.update(Msg::Key(Key::Enter));
    app.update(Msg::Sent(Ok(handle())));
    app
}

#[test]
fn connecting_asks_for_the_session_providers_and_policy() {
    let mut app = App::new(FOLDER);
    assert_eq!(
        app.update(Msg::Connected),
        [Request::Open, Request::Providers, Request::Policy]
    );
}

#[test]
fn a_folder_without_access_asks_before_the_welcome_screen() {
    let mut app = App::new(FOLDER);
    app.update(Msg::Opened(Ok(CliSessionDto {
        folder: FOLDER.to_owned(),
        access: WorkingFolderAccess::Missing,
    })));
    assert_eq!(app.phase, Phase::Access);
    app.update(Msg::Key(Key::Down));
    app.update(Msg::Key(Key::Enter));
    assert_eq!(app.phase, Phase::Chat);
    assert_eq!(app.grant, Some(FolderGrantScope::Always));
    assert!(app.exit.is_none());
}

#[test]
fn refusing_folder_access_ends_the_session() {
    let mut app = App::new(FOLDER);
    app.update(Msg::Opened(Ok(CliSessionDto {
        folder: FOLDER.to_owned(),
        access: WorkingFolderAccess::Missing,
    })));
    app.update(Msg::Key(Key::Esc));
    assert_eq!(
        app.exit,
        Some(Exit::Message(copy::ACCESS_DENIED.to_owned()))
    );
}

#[test]
fn the_default_model_follows_the_provider_default() {
    let app = ready();
    let model = app.model.expect("a model");
    assert_eq!(model.model, "openai/gpt-4o-mini");
    assert_eq!(model.provider_name, "OpenRouter");
    assert_eq!(model.context_window, Some(128_000));
}

#[test]
fn the_first_message_creates_binds_and_then_sends() {
    let mut app = ready();
    app.grant = Some(FolderGrantScope::Conversation);
    typed(&mut app, "привет");
    let requests = app.update(Msg::Key(Key::Enter));
    let Some(Request::Create { model, .. }) = requests.first() else {
        panic!("creates: {requests:?}");
    };
    assert_eq!(model, "openai/gpt-4o-mini");
    assert_eq!(app.transcript.entries, [Entry::User("привет".to_owned())]);

    let created = conversation();
    let id = created.id;
    assert_eq!(
        app.update(Msg::Created(Ok(created.clone()))),
        [Request::Bind {
            id,
            grant: Some(FolderGrantScope::Conversation)
        }]
    );
    let sent = app.update(Msg::Bound(Ok(BindFolderResultDto::Bound {
        conversation: Box::new(created),
    })));
    let [Request::Send(input)] = sent.as_slice() else {
        panic!("sends: {sent:?}");
    };
    assert_eq!(input.text, "привет");
    assert_eq!(input.conversation_id, id);
    assert!(app.grant.is_none());
    assert!(app.busy());
}

#[test]
fn events_that_arrive_before_the_handle_are_replayed() {
    let mut app = ready();
    app.conversation = Some(conversation());
    typed(&mut app, "привет");
    app.update(Msg::Key(Key::Enter));
    app.update(event(0, json!({ "type": "token", "delta": "Здрав" })));
    app.update(Msg::Sent(Ok(handle())));
    app.update(event(1, json!({ "type": "token", "delta": "ствуйте" })));
    app.update(end("ok"));
    assert!(!app.busy());
    assert_eq!(
        app.transcript.entries.last(),
        Some(&Entry::Assistant {
            text: "Здравствуйте".to_owned(),
            reasoning: String::new()
        })
    );
}

#[test]
fn a_failed_run_shows_its_message_and_a_cancel_is_sent_once() {
    let mut app = running();
    assert_eq!(
        app.update(Msg::Key(Key::Esc)),
        [Request::Cancel(handle().id)]
    );
    assert!(app.update(Msg::Key(Key::Esc)).is_empty());
    app.update(event(
        5,
        json!({ "type": "end", "outcome": { "status": "failed", "message": "Лимит" } }),
    ));
    assert!(!app.cancelling);
    assert!(matches!(
        app.transcript.entries.last(),
        Some(Entry::Notice { tone: Tone::Error, text }) if text == "Лимит"
    ));
}

#[test]
fn always_allow_writes_a_workspace_rule_and_approves() {
    let mut app = running();
    app.mode = ExecMode::AutoBypass;
    app.update(event(
        2,
        json!({
            "type": "approval",
            "request": {
                "id": APPROVAL, "runId": RUN, "kind": "process", "subject": "commands.run",
                "scope": "global", "expiresAt": 1,
                "conversationId": "0199aa11-3333-7111-8111-111111111111",
                "tool": "run_command", "action": "Run",
                "process": { "program": "run_command", "argv": ["deploytool", "sync"], "cwd": FOLDER, "packages": [] }
            }
        }),
    ));
    assert_eq!(app.prompts.len(), 1);
    app.update(Msg::Key(Key::Down));
    let requests = app.update(Msg::Key(Key::Enter));
    assert_eq!(
        requests,
        [
            Request::AllowRule(AllowRuleInput::Process {
                argv: vec!["deploytool".to_owned(), "sync".to_owned()],
                cwd: FOLDER.to_owned(),
                folder: Some(FOLDER.to_owned()),
            }),
            Request::Approve {
                run: handle().id,
                approval: APPROVAL.to_owned()
            }
        ]
    );
    assert!(app.prompts.is_empty());
}

#[test]
fn a_prompt_left_open_is_dropped_when_the_run_ends() {
    let mut app = running();
    app.update(event(
        2,
        json!({ "type": "approval", "request": { "id": APPROVAL, "runId": RUN, "subject": "web.fetch" } }),
    ));
    assert_eq!(app.prompts.len(), 1);
    app.update(end("cancelled"));
    assert!(app.prompts.is_empty());
}

#[test]
fn a_message_while_busy_is_kept_in_the_input() {
    let mut app = running();
    typed(&mut app, "ещё");
    assert!(app.update(Msg::Key(Key::Enter)).is_empty());
    assert_eq!(app.input.text(), "ещё");
    assert_eq!(app.status, Some((Tone::Info, copy::BUSY.to_owned())));
}

#[test]
fn the_palette_completes_and_runs_commands() {
    let mut app = ready();
    typed(&mut app, "/ex");
    assert!(app.palette_open());
    app.update(Msg::Key(Key::Tab));
    assert_eq!(app.input.text(), "/exit");
    app.update(Msg::Key(Key::Enter));
    assert_eq!(app.exit, Some(Exit::Quiet));
}

#[test]
fn an_unknown_command_is_reported_and_double_escape_exits() {
    let mut app = ready();
    typed(&mut app, "/nope now");
    app.update(Msg::Key(Key::Enter));
    assert!(matches!(&app.status, Some((Tone::Error, text)) if text.contains("/nope")));
    app.update(Msg::Key(Key::Esc));
    assert!(app.exit.is_none());
    app.update(Msg::Key(Key::Esc));
    assert_eq!(app.exit, Some(Exit::Quiet));
}

#[test]
fn new_starts_a_fresh_conversation_in_the_same_folder() {
    let mut app = ready();
    app.conversation = Some(conversation());
    app.transcript.push_user("старое");
    typed(&mut app, "/new");
    app.update(Msg::Key(Key::Enter));
    assert!(app.conversation.is_none());
    assert!(app.show_welcome());
    assert_eq!(app.folder, FOLDER);
}

#[test]
fn a_lost_connection_exits_with_a_message() {
    let mut app = running();
    app.update(Msg::Closed);
    assert_eq!(app.exit, Some(Exit::Message(copy::DISCONNECTED.to_owned())));
}

fn command(app: &mut App, text: &str) -> Vec<Request> {
    typed(app, text);
    app.update(Msg::Key(Key::Enter))
}

#[test]
fn models_opens_a_panel_and_enter_picks_the_model() {
    let mut app = ready();
    app.model = None;
    assert!(command(&mut app, "/models").is_empty());
    assert!(app.overlay.is_some());
    app.update(Msg::Key(Key::Enter));
    assert!(app.overlay.is_none());
    assert_eq!(
        app.model.map(|model| model.model),
        Some("openai/gpt-4o-mini".to_owned())
    );
}

#[test]
fn disabling_confirmations_needs_a_second_enter() {
    let mut app = ready();
    assert_eq!(command(&mut app, "/permissions"), [Request::Policy]);
    app.update(Msg::Policy(Ok(ExecPolicyDto {
        mode: ExecMode::SafetyAsk,
        judge: None,
    })));
    app.update(Msg::Key(Key::Down));
    app.update(Msg::Key(Key::Down));
    assert!(app.update(Msg::Key(Key::Enter)).is_empty());
    assert_eq!(
        app.update(Msg::Key(Key::Enter)),
        [Request::SetPolicy(ExecPolicyDto {
            mode: ExecMode::NoRestrictions,
            judge: None
        })]
    );
}

#[test]
fn resuming_loads_the_conversation_and_reattaches() {
    let mut app = ready();
    assert_eq!(command(&mut app, "/resume"), [Request::Conversations]);
    let mut own = conversation();
    own.working_folder = Some(FOLDER.to_owned());
    let id = own.id;
    app.update(Msg::Conversations(Ok(vec![own.clone()])));
    assert_eq!(app.update(Msg::Key(Key::Enter)), [Request::Load(id)]);
    let detail = ConversationDetailDto {
        conversation: own,
        messages: Vec::new(),
    };
    assert_eq!(app.update(Msg::Loaded(Ok(detail))), [Request::Attach(id)]);
    assert!(app.overlay.is_none());
    assert_eq!(
        app.conversation.map(|conversation| conversation.id),
        Some(id)
    );
}

#[test]
fn typing_in_a_panel_filters_and_escape_clears_then_closes() {
    let mut app = ready();
    command(&mut app, "/models");
    typed(&mut app, "zzz");
    let overlay = app.overlay.clone().expect("open");
    assert!(overlay.rows(&app.view()).is_empty());
    app.update(Msg::Key(Key::Esc));
    assert!(
        app.overlay
            .as_ref()
            .is_some_and(|overlay| overlay.filter.is_empty())
    );
    app.update(Msg::Key(Key::Esc));
    assert!(app.overlay.is_none());
}

#[test]
fn a_browser_check_is_shown_brought_forward_and_cleared() {
    let mut app = running();
    let notice = |shown| {
        Msg::Notice(zvs_contract::CliNotice::AccountVerification {
            origin: "https://chat.qwen.ai".to_owned(),
            shown,
        })
    };
    assert_eq!(app.update(notice(true)), [Request::FocusVerification]);
    assert_eq!(app.verification.as_deref(), Some("https://chat.qwen.ai"));
    assert_eq!(
        app.update(Msg::Key(Key::Enter)),
        [Request::FocusVerification]
    );
    assert!(app.update(notice(false)).is_empty());
    assert!(app.verification.is_none());
}

#[test]
fn a_bang_runs_a_shell_command_that_esc_can_stop() {
    let mut app = ready();
    typed(&mut app, "!dir /b");
    assert!(app.shell_mode());
    assert_eq!(
        app.update(Msg::Key(Key::Enter)),
        [Request::Shell("dir /b".to_owned())]
    );
    assert!(!app.shell_mode());
    app.update(Msg::ShellOutput("a.txt".to_owned()));
    assert_eq!(app.update(Msg::Key(Key::Esc)), [Request::StopShell]);
    app.update(Msg::ShellEnded(transcript::ShellEnd::Stopped));
    assert!(app.shell.is_none());
    let Some(Entry::Shell(run)) = app.transcript.entries.last() else {
        panic!("a shell entry");
    };
    assert_eq!(run.output, ["a.txt"]);
    assert_eq!(run.end, Some(transcript::ShellEnd::Stopped));
}

fn chord(app: &mut App, chord: shortcuts::Chord) -> Vec<Request> {
    app.update(Msg::Key(Key::Chord(chord)))
}

#[test]
fn a_question_mark_in_an_empty_line_toggles_the_shortcut_panel() {
    let mut app = ready();
    typed(&mut app, "?");
    assert!(app.keys_open());
    assert!(app.input.is_empty());
    let columns = app.key_columns();
    assert_eq!(
        columns.iter().map(|(title, _)| *title).collect::<Vec<_>>(),
        [
            copy::KEYS_COMPOSE,
            copy::KEYS_SESSION,
            copy::KEYS_TRANSCRIPT
        ]
    );
    assert!(
        columns[2]
            .1
            .contains(&("Ctrl+O".to_owned(), copy::KEY_DETAILS))
    );
    typed(&mut app, "?");
    assert!(!app.keys_open());
    assert!(app.input.is_empty());

    typed(&mut app, "?");
    app.update(Msg::Key(Key::Esc));
    assert!(!app.keys_open());
    assert!(app.input.is_empty());
    assert!(!app.escape_armed);
}

#[test]
fn typing_closes_the_shortcut_panel_and_keeps_the_text() {
    let mut app = ready();
    typed(&mut app, "?");
    typed(&mut app, "why?");
    assert!(!app.keys_open());
    assert_eq!(app.input.text(), "why?");
    app.update(Msg::Key(Key::Enter));
    assert!(matches!(app.transcript.entries.last(), Some(Entry::User(text)) if text == "why?"));
    assert!(
        app.transcript
            .entries
            .iter()
            .all(|entry| matches!(entry, Entry::User(_)))
    );
}

#[test]
fn the_theme_panel_previews_the_selection_and_saves_on_enter() {
    let mut app = ready();
    typed(&mut app, "/theme");
    assert!(app.update(Msg::Key(Key::Enter)).is_empty());
    assert_eq!(
        app.overlay.as_ref().map(|overlay| overlay.panel),
        Some(Panel::Theme)
    );
    assert_eq!(app.active_theme(), CliTheme::Ember);
    app.update(Msg::Key(Key::Down));
    assert_eq!(app.active_theme(), CliTheme::Graphite);
    assert_eq!(app.theme, CliTheme::Ember);

    app.update(Msg::Key(Key::Esc));
    assert!(app.overlay.is_none());
    assert_eq!(app.active_theme(), CliTheme::Ember);

    typed(&mut app, "/theme");
    app.update(Msg::Key(Key::Enter));
    app.update(Msg::Key(Key::Down));
    app.update(Msg::Key(Key::Down));
    assert_eq!(
        app.update(Msg::Key(Key::Enter)),
        [Request::SaveTheme(CliTheme::Moss)]
    );
    assert!(app.overlay.is_none());
    assert_eq!(app.theme, CliTheme::Moss);
    assert_eq!(app.active_theme(), CliTheme::Moss);

    app.update(Msg::ThemeSaved(false));
    assert_eq!(
        app.status.as_ref().map(|(_, text)| text.as_str()),
        Some(copy::THEME_SAVE_FAILED)
    );
    app.update(Msg::ThemeChanged(CliTheme::Frost));
    assert_eq!(app.active_theme(), CliTheme::Frost);
}

#[test]
fn chords_run_their_commands_and_unbound_ones_do_nothing() {
    let mut app = ready();
    assert!(chord(&mut app, shortcuts::Chord::ctrl('p')).is_empty());
    assert_eq!(
        app.overlay.as_ref().map(|overlay| overlay.panel),
        Some(Panel::Models)
    );
    assert_eq!(
        chord(&mut app, shortcuts::Chord::alt('s')),
        [Request::Skills]
    );
    assert_eq!(
        app.overlay.as_ref().map(|overlay| overlay.panel),
        Some(Panel::Skills)
    );
    chord(&mut app, shortcuts::Chord::function(1));
    assert!(app.help && app.overlay.is_none());
    assert!(chord(&mut app, shortcuts::Chord::ctrl('x')).is_empty());

    app.help = false;
    chord(&mut app, shortcuts::Chord::ctrl('o'));
    assert!(app.expanded);
    typed(&mut app, "draft");
    chord(&mut app, shortcuts::Chord::ctrl('u'));
    assert!(app.input.is_empty());
}

#[test]
fn the_shortcuts_panel_rebinds_moves_unbinds_and_restores_and_saves_each_change() {
    let mut app = ready();
    typed(&mut app, "/shortcuts");
    app.update(Msg::Key(Key::Enter));
    assert_eq!(
        app.overlay.as_ref().map(|overlay| overlay.panel),
        Some(Panel::Shortcuts)
    );

    app.update(Msg::Key(Key::Enter));
    assert!(app.capturing());
    app.update(Msg::Key(Key::Char('x')));
    assert!(app.capturing());
    assert_eq!(
        app.status.as_ref().map(|(_, text)| text.as_str()),
        Some(copy::SHORTCUT_INVALID)
    );

    let saved = chord(&mut app, shortcuts::Chord::ctrl('p'));
    assert!(!app.capturing());
    assert!(matches!(saved.as_slice(), [Request::SaveShortcuts(_)]));
    assert_eq!(
        app.keymap.action(shortcuts::Chord::ctrl('p')),
        Some(shortcuts::Shortcut::Details)
    );
    assert_eq!(app.keymap.chord(shortcuts::Shortcut::Models), None);
    let status = app.status.clone().map(|(_, text)| text).unwrap_or_default();
    assert!(status.contains(copy::SHORTCUT_MOVED), "{status}");

    app.update(Msg::Key(Key::Enter));
    assert!(matches!(
        app.update(Msg::Key(Key::Backspace)).as_slice(),
        [Request::SaveShortcuts(_)]
    ));
    assert_eq!(app.keymap.chord(shortcuts::Shortcut::Details), None);

    app.update(Msg::Key(Key::Enter));
    app.update(Msg::Key(Key::Esc));
    assert!(!app.capturing());
    assert!(app.overlay.is_some());

    app.update(Msg::Key(Key::Up));
    let reset = app.update(Msg::Key(Key::Enter));
    let [Request::SaveShortcuts(value)] = reset.as_slice() else {
        panic!("a save after the reset");
    };
    assert_eq!(app.keymap, shortcuts::Keymap::default());
    assert_eq!(
        shortcuts::Keymap::from_json(value),
        shortcuts::Keymap::default()
    );

    app.update(Msg::ShortcutsSaved(false));
    assert_eq!(
        app.status.as_ref().map(|(_, text)| text.as_str()),
        Some(copy::SHORTCUTS_SAVE_FAILED)
    );
}

#[test]
fn chords_are_ignored_while_a_prompt_waits() {
    let mut app = App::new(FOLDER);
    app.update(Msg::Opened(Ok(CliSessionDto {
        folder: FOLDER.to_owned(),
        access: WorkingFolderAccess::Missing,
    })));
    assert!(chord(&mut app, shortcuts::Chord::ctrl('p')).is_empty());
    assert!(app.overlay.is_none());
}

fn reasoning_app() -> App {
    let mut provider = provider();
    provider.models[0]
        .capabilities
        .push(zvs_contract::ModelCapability::AdaptiveReasoning);
    let mut app = App::new(FOLDER);
    app.update(Msg::Opened(Ok(CliSessionDto {
        folder: FOLDER.to_owned(),
        access: WorkingFolderAccess::Granted,
    })));
    app.update(Msg::Providers(Ok(vec![provider])));
    app
}

#[test]
fn a_resume_argument_loads_the_conversation_once_the_folder_is_open() {
    let id = ConversationId::parse("0199aa11-2222-7111-8111-0000000000aa").expect("id");
    let mut app = App::new(FOLDER);
    app.resume = Some(id);
    assert_eq!(
        app.update(Msg::Opened(Ok(CliSessionDto {
            folder: FOLDER.to_owned(),
            access: WorkingFolderAccess::Granted,
        }))),
        [Request::Load(id)]
    );
    assert!(app.resume.is_none());
}

#[test]
fn a_resume_argument_waits_for_the_folder_grant() {
    let id = ConversationId::parse("0199aa11-2222-7111-8111-0000000000aa").expect("id");
    let mut app = App::new(FOLDER);
    app.resume = Some(id);
    assert!(
        app.update(Msg::Opened(Ok(CliSessionDto {
            folder: FOLDER.to_owned(),
            access: WorkingFolderAccess::Missing,
        })))
        .is_empty()
    );
    app.update(Msg::Key(Key::Down));
    assert_eq!(app.update(Msg::Key(Key::Enter)), [Request::Load(id)]);
}

#[test]
fn a_failed_resume_argument_exits_with_the_error() {
    let mut app = App::new(FOLDER);
    app.update(Msg::Opened(Ok(CliSessionDto {
        folder: FOLDER.to_owned(),
        access: WorkingFolderAccess::Granted,
    })));
    app.update(Msg::Loaded(Err(AppError::new(
        zvs_contract::AppErrorCode::NotFound,
        "Диалог не найден",
    ))));
    assert_eq!(app.exit, Some(Exit::Message("Диалог не найден".to_owned())));
}

#[test]
fn the_effort_panel_offers_the_steps_the_model_supports() {
    let mut app = reasoning_app();
    assert!(command(&mut app, "/effort").is_empty());
    let overlay = app.overlay.clone().expect("overlay");
    assert_eq!(overlay.panel, Panel::Effort);
    let rows = overlay.rows(&app.view());
    assert_eq!(rows.len(), 3);
    assert_eq!(
        rows.iter()
            .map(|row| row.action.clone())
            .collect::<Vec<_>>(),
        [
            Some(Action::SetEffort(ThinkingEffort::Fast)),
            Some(Action::SetEffort(ThinkingEffort::Auto)),
            Some(Action::SetEffort(ThinkingEffort::Thinking)),
        ]
    );
    assert!(rows[1].current);
    assert_eq!(overlay.selected, 1);
    app.update(Msg::Key(Key::Down));
    app.update(Msg::Key(Key::Enter));
    assert_eq!(app.effort, ThinkingEffort::Thinking);
    assert!(app.overlay.is_none());
}

#[test]
fn a_model_without_reasoning_gets_no_effort_steps() {
    let mut app = ready();
    assert!(command(&mut app, "/effort").is_empty());
    let overlay = app.overlay.clone().expect("overlay");
    let rows = overlay.rows(&app.view());
    assert_eq!(rows.len(), 1);
    assert!(rows[0].action.is_none());
    assert_eq!(rows[0].title, copy::EFFORT_UNSUPPORTED);
}

#[test]
fn the_chosen_effort_is_sent_with_the_message() {
    let mut app = reasoning_app();
    app.effort = ThinkingEffort::Fast;
    let Request::Send(input) = app.send(conversation().id, "привет".to_owned()) else {
        panic!("a send request");
    };
    assert_eq!(input.vendor.effort, Some(ThinkingEffort::Fast));
}

#[test]
fn the_effort_shortcut_opens_the_panel() {
    let mut app = reasoning_app();
    assert!(chord(&mut app, shortcuts::Chord::alt('e')).is_empty());
    assert_eq!(
        app.overlay.as_ref().map(|overlay| overlay.panel),
        Some(Panel::Effort)
    );
}
