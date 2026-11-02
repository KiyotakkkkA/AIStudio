use serde_json::{Value, json};

use super::shortcuts::ChordKey;
use super::transcript::Entry;
use super::*;

const PROVIDERS: &str =
    include_str!("../../../../packages/shared/fixtures/providers/ProviderDto.valid.json");

#[test]
fn connect_lists_the_four_account_providers_in_order() {
    let mut app = ready();
    typed(&mut app, "/connect");
    assert!(app.update(Msg::Key(Key::Enter)).is_empty());
    let overlay = app.overlay.as_ref().expect("panel");
    assert_eq!(overlay.panel, Panel::Connect);
    let titles: Vec<String> = overlay
        .rows(&app.view())
        .into_iter()
        .map(|row| row.title)
        .collect();
    assert_eq!(titles, ["Qwen", "Z.ai", "OpenAI", "GitHub Copilot"]);
}

#[test]
fn a_connected_family_is_marked_in_the_panel() {
    let mut app = ready();
    let mut qwen = provider();
    qwen.summary.adapter = zvs_contract::AdapterFamily::QwenWeb;
    qwen.summary.enabled = true;
    app.update(Msg::Providers(Ok(vec![qwen])));
    let rows = Overlay::new(Panel::Connect).rows(&app.view());
    let marked: Vec<bool> = rows
        .iter()
        .map(|row| row.meta == copy::CONNECT_CONNECTED)
        .collect();
    let available = app.providers[0].models.iter().any(|model| model.available);
    assert_eq!(marked, [available, false, false, false]);
}

#[test]
fn connecting_an_account_can_be_cancelled_and_adopts_its_models_after_completion() {
    let mut app = ready();
    typed(&mut app, "/connect");
    app.update(Msg::Key(Key::Enter));
    app.update(Msg::Key(Key::Down));
    assert_eq!(
        app.update(Msg::Key(Key::Enter)),
        vec![Request::LinkAccount(AccountFamily::ZaiWeb)]
    );
    assert!(app.overlay.is_none());
    assert_eq!(app.linking, Some(AccountFamily::ZaiWeb));
    assert!(
        app.status
            .as_ref()
            .unwrap()
            .1
            .starts_with(copy::CONNECT_LINKING)
    );
    assert!(app.update(Msg::Key(Key::Char('x'))).is_empty());
    assert_eq!(
        app.update(Msg::Key(Key::Esc)),
        vec![Request::CancelLink(AccountFamily::ZaiWeb)]
    );
    let mut zai = provider();
    zai.summary.adapter = zvs_contract::AdapterFamily::ZaiWeb;
    zai.summary.enabled = true;
    app.update(Msg::AccountLinked(
        AccountFamily::ZaiWeb,
        Ok(vec![zai.clone()]),
    ));
    assert_eq!(app.linking, None);
    assert_eq!(app.model.as_ref().unwrap().provider, zai.summary.id);
    assert_eq!(app.effort, ThinkingEffort::Auto);
    assert!(
        app.status
            .as_ref()
            .unwrap()
            .1
            .ends_with(copy::CONNECT_LINKED)
    );
}

#[test]
fn a_failed_connection_releases_the_input() {
    let mut app = ready();
    app.update(Msg::Key(Key::Char('/')));
    app.linking = Some(AccountFamily::Copilot);
    app.update(Msg::AccountLinked(
        AccountFamily::Copilot,
        Err(AppError::new(
            zvs_contract::AppErrorCode::RunCancelled,
            "Отменено",
        )),
    ));
    assert_eq!(app.linking, None);
    typed(&mut app, "x");
    assert_eq!(app.input.text(), "/x");
}

#[test]
fn desktop_opens_the_current_conversation_or_explains_why_not() {
    let mut app = ready();
    typed(&mut app, "/desktop");
    assert!(app.update(Msg::Key(Key::Enter)).is_empty());
    assert_eq!(
        app.status.as_ref().unwrap().1,
        copy::DESKTOP_NO_CONVERSATION
    );
    let known = conversation();
    app.conversation = Some(known.clone());
    typed(&mut app, "/desktop");
    assert_eq!(
        app.update(Msg::Key(Key::Enter)),
        vec![Request::OpenDesktop(known.id)]
    );
    app.update(Msg::DesktopOpened(true));
    assert_eq!(app.status.as_ref().unwrap().1, copy::DESKTOP_OPENED);
    app.update(Msg::DesktopOpened(false));
    assert_eq!(app.status.as_ref().unwrap().1, copy::DESKTOP_FAILED);
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
fn a_message_while_busy_goes_to_the_queue() {
    let mut app = running();
    typed(&mut app, "ещё");
    assert_eq!(
        app.update(Msg::Key(Key::Enter)),
        [Request::QueueAdd {
            id: conversation().id,
            text: "ещё".to_owned()
        }]
    );
    assert_eq!(app.input.text(), "");
    assert_eq!(app.status, Some((Tone::Info, copy::QUEUE_ADDED.to_owned())));
}

fn queued(id: &str, text: &str) -> ChatQueuedMessageDto {
    ChatQueuedMessageDto {
        id: id.to_owned(),
        text: text.to_owned(),
        created_at: 1,
    }
}

fn queue_step(seq: u64, messages: &[ChatQueuedMessageDto], urgent: bool) -> Msg {
    event(
        seq,
        json!({
            "type": "step",
            "step": {
                "domain": "queue",
                "conversationId": conversation().id,
                "messages": messages,
                "urgent": urgent,
            },
        }),
    )
}

fn alt(key: ChordKey) -> Msg {
    Msg::Key(Key::Chord(Chord::alt_key(key)))
}

#[test]
fn the_queue_follows_host_steps_and_its_shortcuts_select_edit_remove_and_send() {
    let mut app = running();
    let id = conversation().id;
    app.update(queue_step(
        3,
        &[queued("q1", "Первое"), queued("q2", "Второе")],
        false,
    ));
    assert_eq!(app.queue.len(), 2);
    assert_eq!(app.queue_selected, None);

    app.update(alt(ChordKey::Up));
    assert_eq!(app.queue_selected, Some(1));
    app.update(alt(ChordKey::Up));
    app.update(alt(ChordKey::Up));
    assert_eq!(app.queue_selected, Some(0));
    app.update(alt(ChordKey::Down));
    assert_eq!(app.queue_selected, Some(1));

    typed(&mut app, "черновик");
    app.update(Msg::Key(Key::Chord(Chord::alt('i'))));
    assert_eq!(app.input.text(), "Второе");
    assert_eq!(app.status.as_ref().unwrap().1, copy::QUEUE_EDITING);
    app.update(Msg::Key(Key::Esc));
    assert_eq!(app.input.text(), "черновик");
    assert!(app.queue_editing.is_none());

    app.update(Msg::Key(Key::Chord(Chord::alt('i'))));
    app.input.set("Изменённое");
    assert_eq!(
        app.update(Msg::Key(Key::Enter)),
        [Request::QueueEdit {
            id,
            message: "q2".to_owned(),
            text: "Изменённое".to_owned()
        }]
    );
    assert_eq!(app.input.text(), "черновик");

    assert_eq!(
        app.update(Msg::Key(Key::Chord(Chord::alt('r')))),
        [Request::QueueRemove {
            id,
            message: "q2".to_owned()
        }]
    );
    assert_eq!(app.update(alt(ChordKey::Enter)), [Request::QueueSend(id)]);

    app.update(Msg::Queue(Ok(ChatQueueDto {
        conversation_id: id,
        messages: vec![queued("q1", "Первое")],
        urgent: true,
    })));
    assert!(app.queue_urgent);
    assert_eq!(app.queue_selected, Some(0));
}

#[test]
fn an_emptied_queue_shows_the_sent_messages_and_attaches_once_idle() {
    let mut app = running();
    let id = conversation().id;
    app.update(queue_step(
        3,
        &[queued("q1", "Первое"), queued("q2", "Второе")],
        false,
    ));
    app.update(end("ok"));
    assert_eq!(app.update(queue_step(4, &[], false)), [Request::Attach(id)]);
    assert!(app.awaiting);
    let users: Vec<&str> = app
        .transcript
        .entries
        .iter()
        .filter_map(|entry| match entry {
            Entry::User(text) => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(users[users.len() - 2..], ["Первое", "Второе"]);
    assert!(app.queue.is_empty());

    let mut busy = running();
    busy.update(queue_step(3, &[queued("q1", "Первое")], false));
    assert!(busy.update(queue_step(4, &[], false)).is_empty());
}

#[test]
fn an_empty_queue_explains_itself_and_an_unbound_alt_enter_types_a_newline() {
    let mut app = running();
    assert!(app.update(alt(ChordKey::Enter)).is_empty());
    assert_eq!(app.status.as_ref().unwrap().1, copy::QUEUE_EMPTY);
    assert!(app.update(Msg::Key(Key::Chord(Chord::alt('r')))).is_empty());

    app.keymap.unbind(Shortcut::QueueSend);
    typed(&mut app, "a");
    app.update(alt(ChordKey::Enter));
    typed(&mut app, "b");
    assert_eq!(app.input.text(), "a\nb");
}

#[test]
fn a_full_queue_keeps_the_message_in_the_input() {
    let mut app = running();
    let messages: Vec<ChatQueuedMessageDto> = (0..CHAT_QUEUE_MAX)
        .map(|index| queued(&format!("q{index}"), "x"))
        .collect();
    app.update(queue_step(3, &messages, false));
    typed(&mut app, "лишнее");
    assert!(app.update(Msg::Key(Key::Enter)).is_empty());
    assert_eq!(app.input.text(), "лишнее");
    assert_eq!(app.status.as_ref().unwrap().1, copy::QUEUE_FULL);
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
        [Request::SetPolicy(PolicyChange::Mode(
            ExecMode::NoRestrictions
        ))]
    );
}

#[test]
fn a_mode_change_leaves_a_judge_chosen_elsewhere_alone() {
    let mut app = ready();
    command(&mut app, "/permissions");
    app.update(Msg::Policy(Ok(ExecPolicyDto {
        mode: ExecMode::SafetyAsk,
        judge: None,
    })));
    app.update(Msg::Key(Key::Up));
    assert_eq!(
        app.update(Msg::Key(Key::Enter)),
        [Request::SetPolicy(PolicyChange::Mode(ExecMode::AlwaysAsk))]
    );
    let judge = zvs_contract::ExecJudgeDto {
        provider_id: provider().summary.id.to_string(),
        model_id: "openai/gpt-4o-mini".to_owned(),
    };
    app.update(Msg::PolicySet(
        PolicyChange::Mode(ExecMode::AlwaysAsk),
        Ok(ExecPolicyDto {
            mode: ExecMode::AlwaysAsk,
            judge: Some(judge.clone()),
        }),
    ));
    assert_eq!(app.policy.and_then(|policy| policy.judge), Some(judge));
    assert_eq!(
        app.status.map(|status| status.1),
        Some(copy::MODE_SET.to_owned())
    );
}

#[test]
fn each_message_refreshes_the_policy_shared_with_the_studio() {
    let mut app = ready();
    app.conversation = Some(conversation());
    typed(&mut app, "привет");
    let requests = app.update(Msg::Key(Key::Enter));
    assert!(matches!(
        requests.as_slice(),
        [Request::Send(_), Request::Policy]
    ));
}

const SERVERS: &str =
    include_str!("../../../../packages/shared/fixtures/mcp/McpServerDto.valid.json");

fn http_server(status: McpServerStatus, oauth: McpOAuthState) -> McpServerDto {
    let mut server: McpServerDto = first(SERVERS);
    server.name = "docs".to_owned();
    server.transport = zvs_contract::McpTransport::Http;
    server.command = None;
    server.args = Vec::new();
    server.env = zvs_contract::McpEnv::new();
    server.url = Some("https://mcp.example.com/mcp".to_owned());
    server.status = status;
    server.oauth = oauth;
    server.tool_count = 3;
    server
}

#[test]
fn mcp_add_creates_starts_and_signs_in_when_the_server_asks() {
    let mut app = ready();
    let requests = command(&mut app, "/mcp add docs https://mcp.example.com/mcp");
    let [Request::AddServer(input)] = requests.as_slice() else {
        panic!("adds: {requests:?}");
    };
    assert_eq!(input.name, "docs");
    assert_eq!(
        input.url,
        Some(Some("https://mcp.example.com/mcp".to_owned()))
    );

    let stopped = http_server(McpServerStatus::Stopped, McpOAuthState::None);
    let id = stopped.id;
    assert_eq!(
        app.update(Msg::ServerAdded(Ok(stopped))),
        [Request::StartServer(id)]
    );
    assert_eq!(
        app.update(Msg::ServerChanged(Ok(http_server(
            McpServerStatus::Failed,
            McpOAuthState::Required
        )))),
        [Request::SignIn(id)]
    );
    assert_eq!(app.signing, Some(id));
    assert!(app.update(Msg::Key(Key::Char('x'))).is_empty());
    assert_eq!(app.update(Msg::Key(Key::Esc)), [Request::CancelSignIn(id)]);
    app.update(Msg::SignedIn(Ok(http_server(
        McpServerStatus::Connected,
        McpOAuthState::SignedIn,
    ))));
    assert_eq!(app.signing, None);
    let (tone, text) = app.status.expect("status");
    assert_eq!(tone, Tone::Success);
    assert!(text.starts_with(copy::MCP_SIGNED_IN));
}

#[test]
fn a_server_that_needs_no_sign_in_reports_its_tools() {
    let mut app = ready();
    command(&mut app, "/mcp add docs https://mcp.example.com/mcp");
    let server = http_server(McpServerStatus::Stopped, McpOAuthState::None);
    app.update(Msg::ServerAdded(Ok(server)));
    assert!(
        app.update(Msg::ServerChanged(Ok(http_server(
            McpServerStatus::Connected,
            McpOAuthState::None
        ))))
        .is_empty()
    );
    assert_eq!(app.joining, None);
    assert_eq!(app.status.map(|status| status.0), Some(Tone::Success));
}

#[test]
fn malformed_mcp_commands_explain_the_format() {
    let mut app = ready();
    assert!(command(&mut app, "/mcp add docs").is_empty());
    assert_eq!(
        app.status.clone().map(|status| status.1),
        Some(copy::MCP_ADD_USAGE.to_owned())
    );
    assert!(command(&mut app, "/mcp drop docs").is_empty());
    assert_eq!(
        app.status.map(|status| status.1),
        Some(copy::MCP_ADD_UNKNOWN.to_owned())
    );
}

#[test]
fn a_server_waiting_for_sign_in_offers_it_in_the_panel() {
    let mut app = ready();
    command(&mut app, "/mcp");
    let server = http_server(McpServerStatus::Failed, McpOAuthState::Required);
    let id = server.id;
    app.update(Msg::Servers(Ok(vec![server])));
    let rows = app.overlay.as_ref().expect("panel").rows(&app.view());
    assert!(rows[0].meta.starts_with(copy::MCP_SIGN_IN_REQUIRED));
    assert_eq!(app.update(Msg::Key(Key::Enter)), [Request::SignIn(id)]);
    assert!(app.overlay.is_none());
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
    assert_eq!(
        app.update(Msg::Loaded(Ok(detail))),
        [
            Request::Attach(id),
            Request::Processes(id),
            Request::QueueList(id)
        ]
    );
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
    let Request::Send(input) = app.send(conversation().id, "привет".to_owned(), None) else {
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

#[test]
fn the_context_panel_also_asks_for_the_provider_limits() {
    let mut app = ready();
    typed(&mut app, "/context");
    let requests = app.update(Msg::Key(Key::Enter));
    let provider = app.model.as_ref().expect("a model").provider;
    assert!(
        matches!(requests.as_slice(), [Request::Context { .. }, Request::Usage(id)] if *id == provider)
    );
    let usage: zvs_contract::ProviderUsageDto = serde_json::from_value(json!({
        "source": "copilot", "windows": [], "detail": null, "checkedAt": 1
    }))
    .expect("usage");
    app.update(Msg::Usage(Ok(Some(usage.clone()))));
    assert_eq!(
        app.overlay
            .as_ref()
            .and_then(|overlay| overlay.usage.clone()),
        Some(usage)
    );
    app.update(Msg::Usage(Err(AppError::new(
        zvs_contract::AppErrorCode::ProviderUnreachable,
        "x",
    ))));
    assert!(app.status.is_none());
}

#[test]
fn mcp_inspect_asks_for_the_named_server_and_posts_its_card() {
    let mut app = ready();
    assert_eq!(
        command(&mut app, "/mcp inspect My Docs"),
        [Request::InspectServer("My Docs".to_owned())]
    );
    assert_eq!(command(&mut app, "/mcp inspect"), [Request::Servers]);
    assert_eq!(app.input.text(), "/mcp inspect ");
    assert!(app.update(Msg::Key(Key::Enter)).is_empty());
    assert_eq!(
        app.status.clone().map(|status| status.1),
        Some(copy::MCP_INSPECT_USAGE.to_owned())
    );
    let detail: McpServerDetailDto = first(include_str!(
        "../../../../packages/shared/fixtures/mcp/McpServerDetailDto.valid.json"
    ));
    app.update(Msg::Inspected(Ok(detail)));
    assert!(matches!(
        app.transcript.entries.last(),
        Some(Entry::Server(detail)) if detail.server.name == "qdrant"
    ));
}

#[test]
fn choosing_a_subcommand_in_the_palette_fills_it_in_for_its_arguments() {
    let mut app = ready();
    typed(&mut app, "/mcp a");
    assert!(app.palette_open());
    assert!(app.update(Msg::Key(Key::Enter)).is_empty());
    assert_eq!(app.input.text(), "/mcp add ");
    assert!(!app.palette_open());
    app.input.clear();
    typed(&mut app, "/mcp ");
    app.update(Msg::Key(Key::Down));
    app.update(Msg::Key(Key::Tab));
    assert_eq!(app.input.text(), "/mcp inspect ");
}

#[test]
fn server_names_are_fetched_when_the_argument_is_reached_and_enter_inspects() {
    let mut app = ready();
    typed(&mut app, "/mcp inspect");
    assert_eq!(app.update(Msg::Key(Key::Char(' '))), [Request::Servers]);
    assert!(app.palette_open());
    assert!(palette::loading(app.input.text(), &app.sources()));
    let server = http_server(McpServerStatus::Connected, McpOAuthState::None);
    let id = server.id;
    app.update(Msg::Servers(Ok(vec![server])));
    assert!(app.update(Msg::Key(Key::Char('d'))).is_empty());
    assert_eq!(
        app.update(Msg::Key(Key::Enter)),
        [Request::InspectServer(id.to_string())]
    );
    assert!(app.input.is_empty());
    assert_eq!(app.lookup_arg, None);
    typed(&mut app, "/mcp inspect ");
    assert_eq!(app.lookup_arg, Some(palette::Arg::Server));
    assert_eq!(app.lookup_servers, None);
}

#[test]
fn tab_fills_in_a_server_name_without_running_it() {
    let mut app = ready();
    typed(&mut app, "/mcp inspect ");
    app.update(Msg::Servers(Ok(vec![http_server(
        McpServerStatus::Connected,
        McpOAuthState::None,
    )])));
    assert!(app.update(Msg::Key(Key::Tab)).is_empty());
    assert_eq!(app.input.text(), "/mcp inspect docs");
}

#[test]
fn a_theme_is_applied_from_its_argument() {
    let mut app = ready();
    typed(&mut app, "/theme мох");
    assert_eq!(
        app.update(Msg::Key(Key::Enter)),
        [Request::SaveTheme(CliTheme::Moss)]
    );
    typed(&mut app, "/theme frost");
    assert_eq!(
        app.update(Msg::Key(Key::Enter)),
        [Request::SaveTheme(CliTheme::Frost)]
    );
    assert_eq!(app.theme, CliTheme::Frost);
    assert!(command(&mut app, "/theme neon").is_empty());
    assert_eq!(
        app.status.map(|status| status.1),
        Some(format!("{}: neon", copy::THEME_NOT_FOUND))
    );
}

#[test]
fn a_conversation_of_this_folder_resumes_from_its_title() {
    let mut app = ready();
    typed(&mut app, "/resume");
    assert_eq!(
        app.update(Msg::Key(Key::Char(' '))),
        [Request::Conversations]
    );
    let mut own = conversation();
    own.working_folder = Some(FOLDER.to_owned());
    own.title = "План релиза".to_owned();
    let id = own.id;
    app.update(Msg::Conversations(Ok(vec![own])));
    typed(&mut app, "план");
    assert_eq!(app.update(Msg::Key(Key::Enter)), [Request::Load(id)]);
    app.update(Msg::Loaded(Err(AppError::new(
        zvs_contract::AppErrorCode::NotFound,
        "Диалог удалён",
    ))));
    assert!(app.exit.is_none());
    assert_eq!(
        app.status.map(|status| status.1),
        Some("Диалог удалён".to_owned())
    );
}

const SHELL_ONE: &str = "0199aa11-4444-7111-8111-111111111111";
const SHELL_TWO: &str = "0199aa11-4444-7111-8111-222222222222";
const SHELL_STREAM: &str = "0199aa11-5555-7111-8111-111111111111";

fn background(id: &str, owner: &str, status: &str) -> BackgroundShellDto {
    let ended = if status == "running" {
        json!(null)
    } else {
        json!(2_000)
    };
    serde_json::from_value(json!({
        "id": id,
        "ownerId": owner,
        "program": "C:/node/npx.cmd",
        "argv": ["npx", "http-server"],
        "cwd": "C:/work",
        "status": status,
        "exitCode": null,
        "streamId": SHELL_STREAM,
        "startedAt": 1_000,
        "endedAt": ended,
    }))
    .expect("a background shell")
}

fn shell_status(id: &str, owner: &str, status: &str) -> Msg {
    let value = json!({
        "streamId": SHELL_STREAM,
        "seq": 4,
        "ts": 3_000,
        "type": "step",
        "step": { "domain": "shell", "id": id, "ownerId": owner, "status": status, "exitCode": null },
    });
    Msg::Event(Box::new(serde_json::from_value(value).expect("event")))
}

fn with_processes() -> (App, ConversationId) {
    let mut app = ready();
    let known = conversation();
    let id = known.id;
    app.conversation = Some(known);
    let owner = id.to_string();
    app.update(Msg::Processes(
        id,
        Ok(vec![
            background(SHELL_ONE, &owner, "running"),
            background(SHELL_TWO, &owner, "exited"),
        ]),
    ));
    (app, id)
}

#[test]
fn processes_need_a_dialog_and_list_running_ones_first() {
    let mut app = ready();
    typed(&mut app, "/processes");
    assert!(app.update(Msg::Key(Key::Enter)).is_empty());
    assert_eq!(
        app.status.as_ref().unwrap().1,
        copy::PROCESSES_NO_CONVERSATION
    );

    let (mut app, id) = with_processes();
    typed(&mut app, "/processes");
    assert_eq!(
        app.update(Msg::Key(Key::Enter)),
        vec![Request::Processes(id)]
    );
    app.update(Msg::Processes(id, Ok(app.processes.clone())));
    let overlay = app.overlay.as_ref().expect("panel");
    assert_eq!(overlay.panel, Panel::Processes);
    assert!(!overlay.loading);
    assert_eq!(overlay.rows(&app.view()).len(), 1);
    app.update(Msg::Key(Key::Tab));
    let overlay = app.overlay.as_ref().expect("panel");
    assert_eq!(overlay.rows(&app.view()).len(), 2);
}

#[test]
fn a_status_step_updates_the_footer_count_outside_any_turn() {
    let (mut app, id) = with_processes();
    let owner = id.to_string();
    assert_eq!(app.running_processes(), 1);
    app.awaiting = true;
    assert!(
        app.update(shell_status(SHELL_ONE, &owner, "stopped"))
            .is_empty()
    );
    assert!(app.held.is_empty());
    assert_eq!(app.running_processes(), 0);
    assert_eq!(
        app.update(shell_status(
            "0199aa11-4444-7111-8111-333333333333",
            &owner,
            "running"
        )),
        vec![Request::Processes(id)]
    );
    assert!(
        app.update(shell_status(
            SHELL_ONE,
            "0199aa11-1111-7111-8111-999999999999",
            "running"
        ))
        .is_empty()
    );
    assert_eq!(app.running_processes(), 0);
}

#[test]
fn the_output_view_follows_new_lines_and_polls_while_running() {
    let (mut app, id) = with_processes();
    typed(&mut app, "/processes");
    app.update(Msg::Key(Key::Enter));
    app.update(Msg::Processes(id, Ok(app.processes.clone())));
    let shell: BackgroundShellId = serde_json::from_value(json!(SHELL_ONE)).expect("an id");
    assert_eq!(
        app.update(Msg::Key(Key::Enter)),
        vec![Request::ProcessOutput {
            id: shell,
            since: None
        }]
    );
    let mut output: ShellOutputDto = serde_json::from_value(json!({
        "shell": app.processes[0],
        "text": "one\ntwo\nthr",
        "nextSeq": 12,
        "truncated": false,
        "eventSeq": 3,
    }))
    .expect("output");
    app.update(Msg::ProcessOutput(Ok(output.clone())));
    output.text = "ee\n".to_owned();
    output.next_seq = 15;
    app.update(Msg::ProcessOutput(Ok(output)));
    let overlay = app.overlay.as_ref().expect("the output view");
    assert_eq!(overlay.panel, Panel::ProcessOutput);
    let lines: Vec<String> = overlay
        .rows(&app.view())
        .into_iter()
        .map(|row| row.title)
        .collect();
    assert_eq!(lines, ["one", "two", "three"]);
    assert_eq!(overlay.selected, 2);

    app.frame = OUTPUT_POLL_TICKS - 1;
    assert_eq!(
        app.update(Msg::Tick),
        vec![Request::ProcessOutput {
            id: shell,
            since: Some(15)
        }]
    );
    app.update(Msg::Key(Key::Up));
    assert!(
        !app.overlay
            .as_ref()
            .unwrap()
            .output
            .as_ref()
            .unwrap()
            .follow
    );
    app.update(Msg::Key(Key::End));
    assert!(
        app.overlay
            .as_ref()
            .unwrap()
            .output
            .as_ref()
            .unwrap()
            .follow
    );
}

#[test]
fn stopping_a_process_takes_a_second_delete() {
    let (mut app, id) = with_processes();
    typed(&mut app, "/processes");
    app.update(Msg::Key(Key::Enter));
    app.update(Msg::Processes(id, Ok(app.processes.clone())));
    assert!(app.update(Msg::Key(Key::Delete)).is_empty());
    assert_eq!(app.status.as_ref().unwrap().1, copy::PROCESS_STOP_CONFIRM);
    let shell: BackgroundShellId = serde_json::from_value(json!(SHELL_ONE)).expect("an id");
    assert_eq!(
        app.update(Msg::Key(Key::Delete)),
        vec![Request::KillProcess(shell)]
    );
    app.update(Msg::ProcessKilled(Ok(background(
        SHELL_ONE,
        &id.to_string(),
        "stopped",
    ))));
    assert_eq!(app.status.as_ref().unwrap().1, copy::PROCESS_STOPPED);
    assert_eq!(app.running_processes(), 0);
}

fn opened_output(app: &mut App, id: ConversationId) -> BackgroundShellId {
    typed(app, "/processes");
    app.update(Msg::Key(Key::Enter));
    app.update(Msg::Processes(id, Ok(app.processes.clone())));
    app.update(Msg::Key(Key::Enter));
    serde_json::from_value(json!(SHELL_ONE)).expect("an id")
}

#[test]
fn the_details_view_goes_back_to_the_list_or_closes() {
    let (mut app, id) = with_processes();
    opened_output(&mut app, id);
    assert_eq!(
        app.overlay.as_ref().expect("details").panel,
        Panel::ProcessOutput
    );
    assert!(app.update(Msg::Key(Key::Char('q'))).is_empty());
    assert!(app.overlay.as_ref().expect("details").filter.is_empty());

    app.update(Msg::Key(Key::Left));
    let overlay = app.overlay.as_ref().expect("the list");
    assert_eq!(overlay.panel, Panel::Processes);
    assert!(!overlay.loading);
    assert_eq!(overlay.rows(&app.view())[overlay.selected].key, SHELL_ONE);
    app.update(Msg::Key(Key::Esc));
    assert!(app.overlay.is_none());

    for key in [Key::Esc, Key::Enter, Key::Char(' ')] {
        opened_output(&mut app, id);
        app.update(Msg::Key(key));
        assert!(app.overlay.is_none());
    }
}

#[test]
fn x_stops_the_shown_process_on_the_second_press() {
    let (mut app, id) = with_processes();
    let shell = opened_output(&mut app, id);
    assert!(app.update(Msg::Key(Key::Char('x'))).is_empty());
    assert_eq!(
        app.status.as_ref().unwrap().1,
        copy::PROCESS_STOP_CONFIRM_KEY
    );
    assert_eq!(
        app.update(Msg::Key(Key::Char('x'))),
        vec![Request::KillProcess(shell)]
    );
    app.update(Msg::ProcessKilled(Ok(background(
        SHELL_ONE,
        &id.to_string(),
        "stopped",
    ))));
    assert!(app.update(Msg::Key(Key::Char('x'))).is_empty());
}

fn goal_step(seq: u64, status: &str, reason: Value, round: u64) -> Msg {
    event(
        seq,
        json!({
            "type": "step",
            "step": {
                "domain": GOAL_STEP_DOMAIN,
                "conversationId": conversation().id,
                "status": status,
                "reason": reason,
                "round": round,
                "usage": { "rounds": round, "tokens": 1200, "elapsedMs": 180_000 },
                "budget": { "rounds": 40, "tokens": 400_000, "minutes": 30 },
            },
        }),
    )
}

fn goal_running() -> App {
    let mut app = ready();
    app.conversation = Some(conversation());
    command(&mut app, "/goal добавить тесты");
    app.update(Msg::Sent(Ok(handle())));
    app
}

#[test]
fn a_goal_is_sent_with_its_command_and_a_bare_goal_only_hints() {
    let mut app = ready();
    app.conversation = Some(conversation());
    assert!(command(&mut app, "/goal").is_empty());
    assert_eq!(app.status.as_ref().unwrap().1, copy::GOAL_HINT);
    assert_eq!(app.input.text(), "/goal ");
    app.input.take();
    let requests = command(&mut app, "/goal добавить тесты");
    let Some(Request::Send(input)) = requests.first() else {
        panic!("sends: {requests:?}");
    };
    assert_eq!(input.commands, [ChatCommand::Goal]);
    assert_eq!(input.text, "добавить тесты");
    assert!(app.goal.is_some_and(|goal| goal.active()));
    let Request::Send(plain) = app.send(conversation().id, "вопрос".to_owned(), None) else {
        panic!("a send");
    };
    assert!(plain.commands.is_empty());
}

#[test]
fn a_goal_for_a_new_dialog_keeps_its_command_until_the_dialog_is_bound() {
    let mut app = ready();
    command(&mut app, "/goal собрать проект");
    assert_eq!(app.pending_command, Some(ChatCommand::Goal));
    let mut created = conversation();
    created.working_folder = Some(FOLDER.to_owned());
    created.working_folder_access = Some(WorkingFolderAccess::Granted);
    app.update(Msg::Created(Ok(created.clone())));
    let requests = app.update(Msg::Bound(Ok(BindFolderResultDto::Bound {
        conversation: Box::new(created),
    })));
    let Some(Request::Send(input)) = requests.first() else {
        panic!("sends: {requests:?}");
    };
    assert_eq!(input.commands, [ChatCommand::Goal]);
    assert_eq!(app.pending_command, None);
}

#[test]
fn goal_steps_drive_the_footer_and_never_become_tool_rows() {
    let mut app = goal_running();
    app.update(goal_step(1, "active", Value::Null, 12));
    let goal = app.goal.expect("a goal");
    assert_eq!(goal.usage.rounds, 12);
    assert_eq!(goal.budget.rounds, 40);
    assert!(
        app.transcript
            .live
            .as_ref()
            .is_some_and(|live| live.segments.is_empty())
    );
    app.frame += 30 * crate::ui::transcript::TICKS_PER_SECOND;
    assert_eq!(app.goal_elapsed_ms(), 210_000);
    app.update(goal_step(2, "paused", json!("tokens"), 13));
    assert!(matches!(
        app.transcript.entries.last(),
        Some(Entry::Notice { text, .. })
            if *text == crate::format::with_reason(copy::GOAL_PAUSED, Some(GoalReason::Tokens))
    ));
    app.frame += 600;
    assert_eq!(app.goal_elapsed_ms(), 180_000);
}

#[test]
fn messages_are_queued_while_a_goal_runs() {
    let mut app = goal_running();
    typed(&mut app, "ещё вопрос");
    assert_eq!(
        app.update(Msg::Key(Key::Enter)),
        [Request::QueueAdd {
            id: conversation().id,
            text: "ещё вопрос".to_owned()
        }]
    );
}

#[test]
fn a_goal_turn_that_yields_to_the_queue_is_not_reported_as_failed() {
    let mut app = goal_running();
    app.update(goal_step(1, "active", Value::Null, 3));
    app.update(queue_step(2, &[queued("q1", "Срочно")], true));
    app.update(end("ok"));
    assert!(app.goal.expect("a goal").active());
    assert!(!app.transcript.entries.iter().any(|entry| matches!(
        entry,
        Entry::Notice { text, .. } if text == copy::GOAL_FAILED
    )));
}

#[test]
fn ctrl_c_during_a_goal_asks_once_then_stops_it_instead_of_leaving() {
    let mut app = goal_running();
    let id = conversation().id;
    assert!(app.update(Msg::Key(Key::Interrupt)).is_empty());
    assert_eq!(app.status.as_ref().unwrap().1, copy::GOAL_STOP_CONFIRM);
    assert!(app.exit.is_none());
    assert_eq!(
        app.update(Msg::Key(Key::Interrupt)),
        [Request::StopGoal(id)]
    );
    assert!(app.exit.is_none());
    let mut stopped = conversation();
    stopped.goal = Some(ChatGoalDto {
        status: GoalStatus::Stopped,
        reason: Some(GoalReason::User),
        ended_at: Some(5),
        ..ChatGoalDto::new("добавить тесты", GoalBudgetDto::default(), 1)
    });
    app.update(Msg::GoalStopped(Ok(stopped)));
    assert!(!app.goal.expect("a goal").shown());
    assert!(matches!(
        app.transcript.entries.last(),
        Some(Entry::Notice { text, .. }) if text == copy::GOAL_STOPPED
    ));
    app.update(end("cancelled"));
    app.update(Msg::Key(Key::Interrupt));
    assert!(app.exit.is_some());
}

#[test]
fn a_key_between_two_ctrl_c_disarms_the_goal_stop() {
    let mut app = goal_running();
    app.update(Msg::Key(Key::Interrupt));
    app.update(Msg::Key(Key::Char('x')));
    assert!(app.update(Msg::Key(Key::Interrupt)).is_empty());
    assert!(app.goal_stop_armed);
}

#[test]
fn goal_subcommands_are_offered_only_where_they_apply() {
    let mut app = ready();
    app.conversation = Some(conversation());
    typed(&mut app, "/goal ");
    assert!(!app.palette_open());
    app.input.take();
    app.goal = Some(GoalState::started(0));
    typed(&mut app, "/goal ");
    let offered = palette::matches(app.input.text(), &app.sources());
    assert_eq!(offered, [Item::Sub(Sub::GoalStop)]);
    app.input.take();
    app.goal = Some(GoalState {
        status: GoalStatus::Paused,
        reason: Some(GoalReason::Rounds),
        ..GoalState::started(0)
    });
    typed(&mut app, "/goal c");
    assert_eq!(
        palette::matches(app.input.text(), &app.sources()),
        [Item::Sub(Sub::GoalContinue)]
    );
    let id = conversation().id;
    assert_eq!(
        app.update(Msg::Key(Key::Enter)),
        [Request::ContinueGoal(id)]
    );
    assert!(app.awaiting);
    assert!(app.goal.is_some_and(|goal| goal.active()));
    assert!(matches!(
        app.transcript.entries.last(),
        Some(Entry::Notice { text, .. }) if text == copy::GOAL_CONTINUED
    ));
}

#[test]
fn typed_goal_subcommands_explain_when_they_do_not_apply() {
    let mut app = ready();
    app.conversation = Some(conversation());
    assert!(command(&mut app, "/goal stop").is_empty());
    assert_eq!(app.status.as_ref().unwrap().1, copy::GOAL_NOT_STOPPABLE);
    assert!(command(&mut app, "/goal continue").is_empty());
    assert_eq!(app.status.as_ref().unwrap().1, copy::GOAL_NOT_PAUSED);
}

#[test]
fn resuming_a_dialog_with_a_paused_goal_restores_the_footer() {
    let mut app = ready();
    let mut own = conversation();
    own.goal = Some(ChatGoalDto {
        status: GoalStatus::Paused,
        reason: Some(GoalReason::Restart),
        ..ChatGoalDto::new("собрать проект", GoalBudgetDto::default(), 1)
    });
    app.update(Msg::Loaded(Ok(ConversationDetailDto {
        conversation: own.clone(),
        messages: Vec::new(),
    })));
    let goal = app.goal.expect("a goal");
    assert_eq!(goal.status, GoalStatus::Paused);
    assert_eq!(goal.reason, Some(GoalReason::Restart));
    own.goal = Some(ChatGoalDto {
        status: GoalStatus::Done,
        summary: Some("Готово".to_owned()),
        ended_at: Some(9),
        ..ChatGoalDto::new("собрать проект", GoalBudgetDto::default(), 1)
    });
    app.update(Msg::Loaded(Ok(ConversationDetailDto {
        conversation: own,
        messages: Vec::new(),
    })));
    assert!(app.goal.is_none());
}

#[test]
fn a_failed_goal_turn_is_reported_and_clears_the_footer() {
    let mut app = goal_running();
    app.update(goal_step(1, "active", Value::Null, 2));
    app.update(end("failed"));
    let goal = app.goal.expect("a goal");
    assert_eq!(goal.status, GoalStatus::Failed);
    assert!(!goal.shown());
    assert!(
        app.transcript
            .entries
            .iter()
            .any(|entry| matches!(entry, Entry::Notice { text, .. } if text == copy::GOAL_FAILED))
    );
}

const CHILD: &str = "0199aa11-8888-7111-8111-111111111111";
const LATER_CHILD: &str = "0199aa11-8888-7111-8111-222222222222";

fn delegate_step(index: u64, id: &str, run: &str, status: &str) -> Msg {
    event(
        10 + index,
        json!({ "type": "step", "step": { "domain": "tool-call", "index": index, "call": {
            "id": id, "key": "agent.delegate", "name": "delegate_task", "round": 0,
            "arguments": { "task": "Найди x", "preset": "explore" },
            "status": if status == "running" { "running" } else { "succeeded" },
            "meta": { "runId": run, "preset": "explore", "label": "Найди x", "task": "Найди x",
                "model": "qwen3:8b", "modelSource": "preferred", "status": status,
                "partial": false, "toolCalls": 0, "tokens": 0 }
        } } }),
    )
}

fn child_step(seq: u64, run: &str, index: u64, status: &str) -> Msg {
    event(
        seq,
        json!({ "type": "step", "step": { "domain": "subagent", "runId": run, "index": index, "call": {
            "id": format!("c{index}"), "key": "fs.read", "name": "fs__read", "round": 0,
            "arguments": { "path": "lib.rs" }, "status": status
        } } }),
    )
}

#[test]
fn child_steps_are_grouped_by_run_inside_the_live_segments() {
    let mut app = running();
    app.update(delegate_step(0, "d1", CHILD, "running"));
    app.update(delegate_step(1, "d2", LATER_CHILD, "running"));
    app.update(child_step(20, CHILD, 0, "running"));
    app.update(child_step(21, CHILD, 0, "succeeded"));
    app.update(child_step(22, CHILD, 1, "running"));
    app.update(child_step(23, LATER_CHILD, 0, "running"));
    let live = app.transcript.live.as_ref().expect("live");
    assert_eq!(live.children[CHILD].len(), 2);
    assert_eq!(
        live.children[CHILD][0].status,
        zvs_contract::ChatToolCallStatus::Succeeded
    );
    assert_eq!(live.children[LATER_CHILD].len(), 1);
    assert_eq!(live.tools().count(), 2);
}

#[test]
fn the_stop_shortcut_ends_the_newest_running_child_only() {
    let mut app = running();
    assert!(
        app.update(Msg::Key(Key::Chord(
            Shortcut::StopSubagent
                .default_chord()
                .expect("bound by default"),
        )))
        .is_empty()
    );
    assert_eq!(
        app.status,
        Some((Tone::Info, copy::SUBAGENT_NONE.to_owned()))
    );
    app.update(delegate_step(0, "d1", CHILD, "running"));
    app.update(delegate_step(1, "d2", LATER_CHILD, "running"));
    let later = zvs_contract::RunId::parse(LATER_CHILD).expect("a run id");
    assert_eq!(
        app.update(Msg::Key(Key::Chord(
            Shortcut::StopSubagent
                .default_chord()
                .expect("bound by default"),
        ))),
        [Request::StopSubagent(later)]
    );
    assert_eq!(
        app.status,
        Some((Tone::Info, copy::SUBAGENT_STOPPING.to_owned()))
    );
    app.update(delegate_step(1, "d2", LATER_CHILD, "cancelled"));
    let first = zvs_contract::RunId::parse(CHILD).expect("a run id");
    typed(&mut app, "/subagent-stop");
    assert_eq!(
        app.update(Msg::Key(Key::Enter)),
        [Request::StopSubagent(first)]
    );
    assert!(app.run.is_some());
}

#[test]
fn a_child_approval_prompt_carries_the_subagent_title() {
    let mut app = running();
    app.update(event(
        5,
        json!({ "type": "approval", "request": {
            "id": APPROVAL, "runId": CHILD, "subject": "demo.drop",
            "subagent": { "runId": CHILD, "label": "Удали временные файлы" }
        } }),
    ));
    let prompt = app.prompts.front().expect("a prompt");
    assert_eq!(prompt.subagent.as_deref(), Some("Удали временные файлы"));
    let PromptKind::Approval { run, .. } = &prompt.kind else {
        panic!("an approval");
    };
    assert_eq!(run.to_string(), CHILD);
    app.update(event(
        6,
        json!({ "type": "approval", "request": {
            "id": "0199aa11-7777-7111-8111-222222222222", "runId": RUN, "subject": "demo.drop",
            "subagent": { "runId": CHILD, "label": "Чужая метка" }
        } }),
    ));
    assert_eq!(app.prompts.back().expect("a prompt").subagent, None);
}

const WAKES: &str =
    include_str!("../../../../packages/shared/fixtures/chat/ChatWakeStepDto.valid.json");

fn wake_step(conversation: ConversationId) -> Msg {
    let mut step: Value = first(WAKES);
    step["message"]["conversationId"] = json!(conversation);
    let value = json!({
        "streamId": STREAM,
        "seq": 3,
        "ts": 3_000,
        "type": "step",
        "step": step,
    });
    Msg::Event(Box::new(serde_json::from_value(value).expect("event")))
}

#[test]
fn a_wake_up_turn_shows_its_card_and_streams_while_the_prompt_is_idle() {
    let mut app = ready();
    let known = conversation();
    let id = known.id;
    app.conversation = Some(known);
    assert_eq!(app.update(wake_step(id)), vec![Request::Attach(id)]);
    assert!(app.transcript.entries.iter().any(|entry| matches!(
        entry,
        Entry::Notice { text, .. } if text.starts_with(copy::PROCESS_ENDED_EVENT)
    )));
    assert!(app.busy());
    app.update(event(5, json!({ "type": "token", "delta": " after" })));
    let mut live: ChatAttachDto = first(include_str!(
        "../../../../packages/shared/fixtures/chat/ChatAttachDto.valid.json"
    ));
    live.run = handle();
    live.seq = Some(4);
    live.content = "Результат".to_owned();
    live.approvals.clear();
    live.questions.clear();
    app.update(Msg::Attached(Ok(Some(live))));
    assert!(!app.awaiting);
    assert_eq!(app.run.as_ref().map(|run| run.id), Some(handle().id));
    assert!(app.transcript.live.is_some());
}

#[test]
fn a_wake_up_of_another_dialog_or_during_a_turn_is_ignored() {
    let mut app = ready();
    let known = conversation();
    let id = known.id;
    app.conversation = Some(known);
    let other = ConversationId::parse("0199aa11-9999-7111-8111-111111111111").expect("an id");
    assert!(app.update(wake_step(other)).is_empty());
    app.run = Some(handle());
    assert!(app.update(wake_step(id)).is_empty());
    assert!(!app.transcript.entries.iter().any(|entry| matches!(
        entry,
        Entry::Notice { text, .. } if text.starts_with(copy::PROCESS_ENDED_EVENT)
    )));
}
