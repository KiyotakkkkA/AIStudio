use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Mutex;

use serde_json::json;

use super::*;
use crate::drivers::ai::mock::direct_http;
use crate::platform::logger::silent_logger;
use crate::testing::{MemoryLogger, TemporaryDirectory, temporary_directory};

const STUB_ARGS: &str = "ZVS_LLAMA_STUB_ARGS";
const STUB_EXIT: &str = "ZVS_LLAMA_STUB_EXIT";
const STUB_READY_FILE: &str = "ZVS_LLAMA_STUB_READY_FILE";
const STUB_TEST: &str = "runtimes::llama_server::tests::llama_stub";

const LLAMA_B10978_RERANK_RESPONSE: &str = r#"{
    "model": "x",
    "object": "list",
    "usage": { "prompt_tokens": 50, "total_tokens": 50 },
    "results": [
        { "index": 1, "relevance_score": 0.9417868852615356 },
        { "index": 2, "relevance_score": -0.22709690034389496 },
        { "index": 0, "relevance_score": -7.699778079986572 }
    ]
}"#;

fn value_of(argv: &[String], flag: &str) -> Option<String> {
    argv.iter()
        .position(|argument| argument == flag)
        .and_then(|index| argv.get(index + 1))
        .cloned()
}

fn respond(stream: &mut TcpStream, status: u16, body: &str) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status} OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.flush();
}

fn handle(stream: TcpStream, key: &str, ready: &dyn Fn() -> bool) {
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }
    let mut length = 0;
    let mut authorised = false;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).is_err() || line == "\r\n" || line.is_empty() {
            break;
        }
        let lower = line.to_lowercase();
        if let Some(value) = lower.strip_prefix("content-length:") {
            length = value.trim().parse().unwrap_or(0);
        }
        if lower.starts_with("authorization:")
            && line.trim_end().ends_with(&format!("Bearer {key}"))
        {
            authorised = true;
        }
    }
    let mut body = vec![0; length];
    let _ = reader.read_exact(&mut body);
    let mut stream = reader.into_inner();
    let path = request_line.split_whitespace().nth(1).unwrap_or("/");
    if path == "/health" {
        if ready() {
            respond(&mut stream, 200, r#"{"status":"ok"}"#);
        } else {
            respond(
                &mut stream,
                503,
                r#"{"error":{"code":503,"message":"Loading model"}}"#,
            );
        }
        return;
    }
    if !authorised {
        respond(&mut stream, 401, r#"{"error":"Invalid API Key"}"#);
        return;
    }
    let request: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    match path {
        "/v1/embeddings" => {
            let texts: Vec<String> = request["input"]
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .map(|item| item.as_str().unwrap_or_default().to_owned())
                        .collect()
                })
                .unwrap_or_default();
            if texts.iter().any(|text| text == "crash") {
                std::process::exit(9);
            }
            if texts.iter().any(|text| text == "refuse") {
                respond(&mut stream, 500, "the stub refused");
                return;
            }
            let data: Vec<Value> = texts
                .iter()
                .enumerate()
                .map(|(index, text)| {
                    json!({ "index": index, "embedding": [text.chars().count() as f64, 0.5] })
                })
                .collect();
            respond(&mut stream, 200, &json!({ "data": data }).to_string());
            if texts.iter().any(|text| text == "exit-after") {
                std::process::exit(0);
            }
        }
        "/v1/rerank" => {
            if request["query"] == "fixture" {
                respond(&mut stream, 200, LLAMA_B10978_RERANK_RESPONSE);
                return;
            }
            let documents = request["documents"].as_array().cloned().unwrap_or_default();
            let results: Vec<Value> = documents
                .iter()
                .enumerate()
                .rev()
                .map(|(index, document)| {
                    json!({
                        "index": index,
                        "relevance_score": document.as_str().unwrap_or_default().len() as f64 - 5.0,
                    })
                })
                .collect();
            respond(&mut stream, 200, &json!({ "results": results }).to_string());
        }
        "/v1/chat/completions" => {
            let image = request["messages"][0]["content"][1]["image_url"]["url"]
                .as_str()
                .unwrap_or_default();
            let prompt = request["messages"][0]["content"][0]["text"]
                .as_str()
                .unwrap_or_default();
            let text = format!(
                "  {} | {}  ",
                &image[..image.find(',').unwrap_or(0)],
                prompt.chars().count()
            );
            respond(
                &mut stream,
                200,
                &json!({ "choices": [{ "message": { "content": text } }] }).to_string(),
            );
        }
        _ => respond(&mut stream, 404, "{}"),
    }
}

#[test]
fn llama_stub() {
    let Ok(raw) = std::env::var(STUB_ARGS) else {
        return;
    };
    let argv: Vec<String> = serde_json::from_str(&raw).expect("the stub arguments");
    if let Ok(code) = std::env::var(STUB_EXIT) {
        eprintln!("llama_model_load: error loading model: failed to load model");
        std::process::exit(code.parse().unwrap_or(1));
    }
    let port: u16 = value_of(&argv, "--port")
        .and_then(|port| port.parse().ok())
        .expect("a port");
    let key = value_of(&argv, "--api-key").expect("a key");
    let ready_file = std::env::var(STUB_READY_FILE).ok();
    let ready = move || {
        ready_file
            .as_ref()
            .is_none_or(|file| Path::new(file).exists())
    };
    let listener = TcpListener::bind(("127.0.0.1", port)).expect("binds the stub port");
    eprintln!("main: server is listening on http://127.0.0.1:{port}");
    for stream in listener.incoming().flatten() {
        handle(stream, &key, &ready);
    }
}

struct Stub {
    spawned: Arc<Mutex<Vec<Vec<String>>>>,
    workspace: TemporaryDirectory,
}

impl Stub {
    fn new() -> Self {
        Self {
            spawned: Arc::new(Mutex::new(Vec::new())),
            workspace: temporary_directory("llama-stub-"),
        }
    }

    fn ready_file(&self) -> PathBuf {
        self.workspace.join("ready")
    }

    fn spawned(&self) -> Vec<Vec<String>> {
        self.spawned.lock().expect("not poisoned").clone()
    }

    fn launcher(&self, env: Vec<(&'static str, String)>) -> Launcher {
        let spawned = Arc::clone(&self.spawned);
        Arc::new(move |_executable: &Path, argv: &[String]| {
            spawned.lock().expect("not poisoned").push(argv.to_vec());
            let mut command = Command::new(std::env::current_exe().expect("the test binary"));
            command
                .args(["--exact", STUB_TEST, "--nocapture", "--test-threads", "1"])
                .env(STUB_ARGS, serde_json::to_string(argv).expect("serialises"));
            for (name, value) in &env {
                command.env(name, value);
            }
            command
        })
    }

    fn server(&self, role: LlamaServerRole, env: Vec<(&'static str, String)>) -> LlamaServer {
        self.server_with(role, env, |options| options)
    }

    fn server_with(
        &self,
        role: LlamaServerRole,
        env: Vec<(&'static str, String)>,
        tune: impl FnOnce(LlamaServerOptions) -> LlamaServerOptions,
    ) -> LlamaServer {
        let options = LlamaServerOptions {
            idle: Duration::ZERO,
            health_interval: Duration::from_millis(25),
            launcher: Some(self.launcher(env)),
            mmproj_path: (role == LlamaServerRole::Vision)
                .then(|| "C:/models/mmproj-Qwen2.5-VL-3B-Instruct-Q8_0.gguf".to_owned()),
            ..LlamaServerOptions::new(
                PathBuf::from("llama-server.exe"),
                "C:/models/bge-reranker-v2-m3-FP16.gguf",
                role,
                999,
                direct_http(),
                silent_logger(),
            )
        };
        LlamaServer::new(tune(options))
    }
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

fn deadline() -> CancellationToken {
    let token = CancellationToken::new();
    let fired = token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(30)).await;
        fired.cancel();
    });
    token
}

async fn wait_until(condition: impl Fn() -> bool) {
    for _ in 0..600 {
        if condition() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the condition never held");
}

#[test]
fn the_rerank_role_starts_llama_server_in_reranking_mode_with_a_full_micro_batch() {
    let options = LlamaServerOptions::new(
        PathBuf::from("llama-server.exe"),
        "C:/models/bge-reranker-v2-m3-FP16.gguf",
        LlamaServerRole::Rerank,
        999,
        direct_http(),
        silent_logger(),
    );
    let argv = options.argv(18733, "key");
    assert!(argv.contains(&"--reranking".to_owned()));
    assert!(!argv.contains(&"--embeddings".to_owned()));
    assert!(!argv.contains(&"--mmproj".to_owned()));
    assert_eq!(
        value_of(&argv, "--ubatch-size"),
        value_of(&argv, "--batch-size")
    );
    assert_eq!(value_of(&argv, "--host").as_deref(), Some("127.0.0.1"));
    assert_eq!(value_of(&argv, "--port").as_deref(), Some("18733"));
    assert_eq!(value_of(&argv, "--n-gpu-layers").as_deref(), Some("999"));
    assert_eq!(value_of(&argv, "--ctx-size").as_deref(), Some("8192"));

    let vision = LlamaServerOptions {
        role: LlamaServerRole::Vision,
        mmproj_path: Some("mmproj.gguf".to_owned()),
        threads: Some(6),
        ..options.clone()
    }
    .argv(1, "key");
    assert_eq!(
        value_of(&vision, "--mmproj").as_deref(),
        Some("mmproj.gguf")
    );
    assert!(!vision.contains(&"--ubatch-size".to_owned()));
    assert_eq!(value_of(&vision, "--threads").as_deref(), Some("6"));
    let embedding = LlamaServerOptions {
        role: LlamaServerRole::Embedding,
        ..options
    }
    .argv(1, "key");
    assert!(embedding.contains(&"--embeddings".to_owned()));
}

#[test]
fn rerank_scores_come_back_in_passage_order_as_the_raw_logits_the_build_answers_with() {
    let payload: Value = serde_json::from_str(LLAMA_B10978_RERANK_RESPONSE).expect("parses");
    assert_eq!(
        read_rerank_scores(&payload, 3).expect("reads"),
        [
            -7.699_778_079_986_572,
            0.941_786_885_261_535_6,
            -0.227_096_900_343_894_96
        ]
    );
    assert_eq!(
        read_rerank_scores(
            &json!({ "results": [{ "index": 0, "relevance_score": 1 }] }),
            2
        )
        .expect_err("refuses")
        .message,
        MISSING_SCORES
    );
    for bad in [
        json!({ "results": [{ "index": 3, "relevance_score": 1 }] }),
        json!({ "results": [{ "index": 0, "relevance_score": "high" }] }),
        json!({ "results": [{ "index": 0.5, "relevance_score": 1 }] }),
    ] {
        assert_eq!(
            read_rerank_scores(&bad, 1).expect_err("refuses").message,
            UNEXPECTED_SCORE
        );
    }
    assert_eq!(
        read_rerank_scores(&json!({}), 1)
            .expect_err("refuses")
            .message,
        UNEXPECTED_ANSWER
    );
}

#[test]
fn embeddings_and_chat_answers_are_read_as_the_endpoints_return_them() {
    assert_eq!(
        read_embeddings(&json!({ "data": [{ "embedding": [0.5, 0.25] }] })).expect("reads"),
        [vec![0.5_f32, 0.25]]
    );
    assert_eq!(
        read_embeddings(&json!({ "data": [{ "embedding": [0.5, "x"] }] }))
            .expect_err("refuses")
            .message,
        EMPTY_VECTOR
    );
    assert_eq!(
        read_chat_text(&json!({ "choices": [{ "message": { "content": "текст" } }] }))
            .expect("reads"),
        "текст"
    );
    assert_eq!(
        read_chat_text(&json!({ "choices": [{ "message": { "content": [
            { "type": "text", "text": "a" }, { "type": "image" }, { "text": "b" }
        ] } }] }))
        .expect("reads"),
        "ab"
    );
    assert_eq!(
        read_chat_text(&json!({ "choices": [] }))
            .expect_err("refuses")
            .message,
        EMPTY_ANSWER
    );
    assert_eq!(
        read_chat_text(&json!({ "choices": [{ "message": {} }] }))
            .expect_err("refuses")
            .message,
        UNKNOWN_ANSWER
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_real_child_answers_rerank_in_passage_order_over_loopback_with_its_key() {
    let stub = Stub::new();
    let server = stub.server(LlamaServerRole::Rerank, Vec::new());

    let scores = server
        .rerank(
            "what is a panda?",
            &strings(&["hi", "panda", "pandas"]),
            &deadline(),
        )
        .await
        .expect("reranks");
    assert_eq!(scores, [-3.0, 0.0, 1.0]);
    let fixture = server
        .rerank("fixture", &strings(&["a", "b", "c"]), &deadline())
        .await
        .expect("reranks");
    assert_eq!(
        fixture,
        [
            -7.699_778_079_986_572,
            0.941_786_885_261_535_6,
            -0.227_096_900_343_894_96
        ]
    );

    let spawned = stub.spawned();
    assert_eq!(spawned.len(), 1);
    assert!(spawned[0].contains(&"--reranking".to_owned()));
    let base_url = server.base_url().expect("a base url");
    assert!(base_url.starts_with("http://127.0.0.1:"));
    let unauthorised = direct_http()
        .client()
        .post(format!("{base_url}/v1/rerank"))
        .body("{}")
        .send()
        .await
        .expect("answers");
    assert_eq!(unauthorised.status().as_u16(), 401);
    server.stop();
    server.stopped().await;
    assert!(!server.is_running());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn embeddings_and_an_image_reading_go_through_their_own_endpoints() {
    let stub = Stub::new();
    let embedder = stub.server(LlamaServerRole::Embedding, Vec::new());
    let vectors = embedder
        .embed(&strings(&["привет", "мир!"]), &deadline())
        .await
        .expect("embeds");
    assert_eq!(vectors, [vec![6.0_f32, 0.5], vec![4.0, 0.5]]);
    assert!(
        embedder
            .embed(&[], &deadline())
            .await
            .expect("embeds")
            .is_empty()
    );

    let reader = stub.server(LlamaServerRole::Vision, Vec::new());
    let text = reader
        .describe_image("image/png", b"\x89PNG", "Прочитай", &deadline())
        .await
        .expect("reads");
    assert_eq!(text, "  data:image/png;base64 | 8  ");
    let spawned = stub.spawned();
    assert_eq!(
        value_of(&spawned[1], "--mmproj").as_deref(),
        Some("C:/models/mmproj-Qwen2.5-VL-3B-Instruct-Q8_0.gguf")
    );

    let refused = embedder
        .embed(&strings(&["refuse"]), &deadline())
        .await
        .expect_err("refuses");
    assert_eq!(refused.code, AppErrorCode::Unknown);
    assert_eq!(
        refused.message,
        "Локальный движок не смог посчитать эмбеддинги"
    );
    assert_eq!(
        refused
            .details
            .and_then(|details| details.get("status").cloned()),
        Some(json!(500))
    );
    assert!(embedder.is_running());
    embedder.stop();
    reader.stop();
    embedder.stopped().await;
    reader.stopped().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn giving_up_on_a_cold_start_does_not_kill_the_model_that_is_still_loading() {
    let stub = Stub::new();
    let ready = stub.ready_file();
    let server = stub.server(
        LlamaServerRole::Rerank,
        vec![(STUB_READY_FILE, ready.to_string_lossy().into_owned())],
    );
    let cancel = CancellationToken::new();
    let first = {
        let server = server.clone();
        let cancel = cancel.clone();
        tokio::spawn(async move {
            server
                .rerank("q", &strings(&["a", "b", "c"]), &cancel)
                .await
        })
    };
    wait_until(|| server.is_running()).await;
    cancel.cancel();
    let error = first.await.expect("joins").expect_err("gives up");
    assert_eq!(error.code, AppErrorCode::RunCancelled);
    assert!(server.is_running());

    std::fs::write(&ready, "").expect("marks the model loaded");
    let scores = server
        .rerank("q", &strings(&["a", "b", "c"]), &deadline())
        .await
        .expect("reranks");
    assert_eq!(scores.len(), 3);
    assert_eq!(stub.spawned().len(), 1);
    server.stop();
    server.stopped().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancelled_embedding_start_takes_the_loading_process_down_with_it() {
    let stub = Stub::new();
    let server = stub.server(
        LlamaServerRole::Embedding,
        vec![(
            STUB_READY_FILE,
            stub.ready_file().to_string_lossy().into_owned(),
        )],
    );
    let cancel = CancellationToken::new();
    let call = {
        let server = server.clone();
        let cancel = cancel.clone();
        tokio::spawn(async move { server.embed(&strings(&["a"]), &cancel).await })
    };
    wait_until(|| server.is_running()).await;
    cancel.cancel();
    assert_eq!(
        call.await.expect("joins").expect_err("cancelled").code,
        AppErrorCode::RunCancelled
    );
    server.stopped().await;
    wait_until(|| !server.is_running()).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_empty_passage_list_costs_no_process_and_no_request() {
    let stub = Stub::new();
    let server = stub.server(LlamaServerRole::Rerank, Vec::new());

    assert!(
        server
            .rerank("q", &[], &deadline())
            .await
            .expect("answers")
            .is_empty()
    );
    assert!(stub.spawned().is_empty());
    assert!(!server.is_running());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_server_that_died_mid_request_is_started_again_on_the_next_call() {
    let stub = Stub::new();
    let server = stub.server(LlamaServerRole::Embedding, Vec::new());

    let error = server
        .embed(&strings(&["crash"]), &deadline())
        .await
        .expect_err("the request fails");
    assert_eq!(error.code, AppErrorCode::ProviderUnreachable);
    assert_eq!(
        server
            .embed(&strings(&["abc"]), &deadline())
            .await
            .expect("restarts"),
        [vec![3.0_f32, 0.5]]
    );
    assert_eq!(stub.spawned().len(), 2);
    server.stop();
    server.stopped().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_process_that_exits_on_its_own_is_not_mistaken_for_a_running_server() {
    let logger = MemoryLogger::shared();
    let stub = Stub::new();
    let server = stub.server_with(LlamaServerRole::Embedding, Vec::new(), |options| {
        LlamaServerOptions {
            logger: logger.clone(),
            ..options
        }
    });

    server
        .embed(&strings(&["exit-after"]), &deadline())
        .await
        .expect("embeds");
    wait_until(|| !server.is_running()).await;
    assert_eq!(server.base_url(), None);
    assert_eq!(logger.matching("A local engine server exited").len(), 1);

    server
        .embed(&strings(&["again"]), &deadline())
        .await
        .expect("starts again");
    assert_eq!(stub.spawned().len(), 2);
    server.stop();
    server.stopped().await;
    assert_eq!(logger.matching("A local engine server exited").len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_build_that_crashes_while_loading_reports_its_exit_code_and_stderr() {
    let stub = Stub::new();
    let server = stub.server(
        LlamaServerRole::Embedding,
        vec![(STUB_EXIT, "3".to_owned())],
    );

    let error = server
        .embed(&strings(&["a"]), &deadline())
        .await
        .expect_err("fails to start");

    assert_eq!(error.code, AppErrorCode::Unknown);
    assert_eq!(error.message, EXITED_AT_START);
    let details = error.details.expect("details");
    assert_eq!(details.get("exitCode"), Some(&json!(3)));
    assert!(
        details
            .get("stderr")
            .and_then(Value::as_str)
            .is_some_and(|stderr| stderr.contains("failed to load model"))
    );
    assert!(!server.is_running());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_server_that_never_turns_healthy_is_stopped_after_the_start_timeout() {
    let stub = Stub::new();
    let server = stub.server_with(
        LlamaServerRole::Embedding,
        vec![(
            STUB_READY_FILE,
            stub.ready_file().to_string_lossy().into_owned(),
        )],
        |options| LlamaServerOptions {
            start_timeout: Duration::from_millis(400),
            ..options
        },
    );

    let error = server
        .embed(&strings(&["a"]), &deadline())
        .await
        .expect_err("times out");

    assert_eq!(error.code, AppErrorCode::ProviderUnreachable);
    assert_eq!(error.message, NO_ANSWER_AT_START);
    server.stopped().await;
    assert!(!server.is_running());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_idle_server_stops_itself_and_a_missing_executable_fails_at_once() {
    let logger = MemoryLogger::shared();
    let stub = Stub::new();
    let server = stub.server_with(LlamaServerRole::Embedding, Vec::new(), |options| {
        LlamaServerOptions {
            idle: Duration::from_millis(200),
            logger: logger.clone(),
            ..options
        }
    });
    server
        .embed(&strings(&["a"]), &deadline())
        .await
        .expect("embeds");
    assert!(server.is_running());
    wait_until(|| !server.is_running()).await;
    assert_eq!(logger.matching("Stopping an idle engine server").len(), 1);
    assert!(logger.matching("A local engine server exited").is_empty());

    let missing = LlamaServer::new(LlamaServerOptions {
        idle: Duration::ZERO,
        ..LlamaServerOptions::new(
            stub.workspace.join("llama-server-missing.exe"),
            "model.gguf",
            LlamaServerRole::Embedding,
            0,
            direct_http(),
            silent_logger(),
        )
    });
    let error = missing
        .embed(&strings(&["a"]), &deadline())
        .await
        .expect_err("cannot spawn");
    assert_eq!(error.message, EXITED_AT_START);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_calls_share_one_starting_process() {
    let stub = Stub::new();
    let server = stub.server(LlamaServerRole::Embedding, Vec::new());
    let calls: Vec<_> = (0..4)
        .map(|index| {
            let server = server.clone();
            tokio::spawn(async move { server.embed(&[format!("text-{index}")], &deadline()).await })
        })
        .collect();
    for call in calls {
        call.await.expect("joins").expect("embeds");
    }
    assert_eq!(stub.spawned().len(), 1);
    server.stop();
    server.stopped().await;
}
