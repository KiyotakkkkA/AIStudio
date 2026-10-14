use std::any::Any;
use std::collections::HashMap;
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::pin::Pin;
use std::sync::Arc;

use futures::FutureExt;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use zvs_contract::{
    AppError, AppErrorCode, CHANNELS, ResultEnvelope, VALIDATION_MESSAGE, Validate,
};

use crate::platform::logger::{LogLevel, SharedLogger};
use crate::services::Services;

pub const INTERNAL_MESSAGE: &str = "Внутренняя ошибка";

enum Failure {
    Input(String),
    Output(String),
    Handler(AppError),
}

type HandlerFuture = Pin<Box<dyn Future<Output = Result<Value, Failure>> + Send>>;
type Handler = Box<dyn Fn(Arc<Services>, Value, bool) -> HandlerFuture + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Coverage {
    pub missing: Vec<&'static str>,
}

impl Coverage {
    pub fn is_complete(&self) -> bool {
        self.missing.is_empty()
    }
}

pub struct IpcServer {
    services: Arc<Services>,
    logger: SharedLogger,
    handlers: HashMap<&'static str, Handler>,
    validate_output: bool,
}

impl IpcServer {
    pub fn new(services: Arc<Services>, logger: SharedLogger) -> Self {
        Self {
            services,
            logger,
            handlers: HashMap::new(),
            validate_output: cfg!(debug_assertions),
        }
    }

    #[must_use]
    pub const fn with_output_validation(mut self, enabled: bool) -> Self {
        self.validate_output = enabled;
        self
    }

    pub fn handle<I, O, F, Fut>(&mut self, channel: &str, handler: F) -> &mut Self
    where
        I: DeserializeOwned + Validate + Send + 'static,
        O: Serialize + Validate + Send + 'static,
        F: Fn(Arc<Services>, I) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<O, AppError>> + Send + 'static,
    {
        let channel = CHANNELS
            .iter()
            .copied()
            .find(|known| *known == channel)
            .unwrap_or_else(|| panic!("«{channel}» is not a contract channel"));
        assert!(
            !self.handlers.contains_key(channel),
            "«{channel}» is registered twice"
        );
        let handler = Arc::new(handler);
        self.handlers.insert(
            channel,
            Box::new(move |services, payload, validate_output| {
                let handler = Arc::clone(&handler);
                Box::pin(async move {
                    let input: I = serde_json::from_value(payload)
                        .map_err(|error| Failure::Input(error.to_string()))?;
                    input
                        .validate()
                        .map_err(|error| Failure::Input(describe_validation(&error)))?;
                    let output = handler(services, input).await.map_err(Failure::Handler)?;
                    if validate_output {
                        output
                            .validate()
                            .map_err(|error| Failure::Output(describe_validation(&error)))?;
                    }
                    serde_json::to_value(ResultEnvelope::Ok(output))
                        .map_err(|error| Failure::Output(error.to_string()))
                })
            }),
        );
        self
    }

    pub fn channels(&self) -> Vec<&'static str> {
        let mut channels: Vec<&'static str> = self.handlers.keys().copied().collect();
        channels.sort_unstable();
        channels
    }

    pub fn coverage(&self) -> Coverage {
        Coverage {
            missing: CHANNELS
                .iter()
                .copied()
                .filter(|channel| !self.handlers.contains_key(channel))
                .collect(),
        }
    }

    pub fn finish(&self) -> Coverage {
        let coverage = self.coverage();
        if coverage.is_complete() {
            self.logger.log(
                LogLevel::Info,
                "ipc",
                "Every contract channel is served",
                &[("count", json!(self.handlers.len()))],
            );
        } else {
            self.logger.log(
                LogLevel::Warn,
                "ipc",
                "Channels not yet ported answer NOT_IMPLEMENTED",
                &[
                    ("served", json!(self.handlers.len())),
                    ("missing", json!(coverage.missing.len())),
                    ("channels", json!(coverage.missing)),
                ],
            );
        }
        coverage
    }

    pub async fn dispatch(&self, channel: &str, payload: Value) -> Value {
        let Some(handler) = self.handlers.get(channel) else {
            self.logger.log(
                LogLevel::Debug,
                "ipc",
                "Called a channel that is not implemented",
                &[("channel", json!(channel))],
            );
            return failure(AppError::new(
                AppErrorCode::NotImplemented,
                format!("Канал «{channel}» ещё не перенесён в Tauri-хост"),
            ));
        };
        let call = handler(Arc::clone(&self.services), payload, self.validate_output);
        match AssertUnwindSafe(call).catch_unwind().await {
            Ok(Ok(envelope)) => envelope,
            Ok(Err(Failure::Input(reason))) => {
                self.logger.log(
                    LogLevel::Warn,
                    "ipc",
                    "Rejected invalid input",
                    &[("channel", json!(channel)), ("reason", json!(reason))],
                );
                failure(AppError::new(
                    AppErrorCode::ValidationFailed,
                    VALIDATION_MESSAGE,
                ))
            }
            Ok(Err(Failure::Output(reason))) => {
                self.logger.log(
                    LogLevel::Error,
                    "ipc",
                    "Handler output does not match the contract",
                    &[("channel", json!(channel)), ("reason", json!(reason))],
                );
                internal()
            }
            Ok(Err(Failure::Handler(error))) => {
                self.logger.log(
                    LogLevel::Error,
                    "ipc",
                    "Handler failed",
                    &[
                        ("channel", json!(channel)),
                        ("code", json!(error.code)),
                        ("error", json!(error.to_string())),
                    ],
                );
                failure(error)
            }
            Err(panic) => {
                self.logger.log(
                    LogLevel::Error,
                    "ipc",
                    "Handler failed",
                    &[
                        ("channel", json!(channel)),
                        ("code", json!(AppErrorCode::Unknown)),
                        ("error", json!(panic_message(panic.as_ref()))),
                    ],
                );
                internal()
            }
        }
    }
}

fn failure(error: AppError) -> Value {
    serde_json::to_value(ResultEnvelope::<()>::Err(error)).unwrap_or_else(
        |_| json!({ "ok": false, "error": { "code": "UNKNOWN", "message": INTERNAL_MESSAGE } }),
    )
}

fn internal() -> Value {
    failure(AppError::new(AppErrorCode::Unknown, INTERNAL_MESSAGE))
}

fn describe_validation(error: &AppError) -> String {
    error.details.as_ref().map_or_else(
        || error.to_string(),
        |details| Value::Object(details.clone()).to_string(),
    )
}

fn panic_message(panic: &(dyn Any + Send)) -> String {
    panic
        .downcast_ref::<&str>()
        .map(|message| (*message).to_owned())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a handler panicked".to_owned())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use zvs_contract::{
        SystemDeviceInput, SystemNativePingInput, SystemNativePingOutput, SystemPingInput,
        SystemPingOutput,
    };

    use super::*;
    use crate::platform::clock::FakeClock;
    use crate::platform::events::EventBus;
    use crate::services::SystemService;
    use crate::testing::{MemoryLogger, TestServices, test_services};

    fn services() -> TestServices {
        test_services(SystemService::new(
            Arc::new(FakeClock::new(1_250)),
            EventBus::default(),
            Duration::ZERO,
        ))
    }

    fn server() -> (TestServices, IpcServer, Arc<MemoryLogger>) {
        let host = services();
        let logger = MemoryLogger::shared();
        let mut server = IpcServer::new(host.shared(), logger.clone()).with_output_validation(true);
        server.handle(
            "system.ping",
            |services: Arc<Services>, input: SystemPingInput| async move {
                Ok(services.system.ping(input.sent_at))
            },
        );
        (host, server, logger)
    }

    fn native_failing(
        server: &mut IpcServer,
        error: impl Fn() -> AppError + Send + Sync + 'static,
    ) {
        server.handle(
            "system.nativePing",
            move |_services: Arc<Services>, _input: SystemNativePingInput| {
                let error = error();
                async move { Err::<SystemNativePingOutput, _>(error) }
            },
        );
    }

    #[tokio::test]
    async fn round_trips_a_valid_call() {
        let (_host, server, _logger) = server();
        assert_eq!(
            server
                .dispatch("system.ping", json!({ "sentAt": 1_000 }))
                .await,
            json!({ "ok": true, "data": { "pong": true, "hostTime": 1_250, "roundTripHint": 250.0 } })
        );
    }

    #[tokio::test]
    async fn rejects_invalid_input_with_validation_failed_before_the_handler_runs() {
        let logger = MemoryLogger::shared();
        let calls = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&calls);
        let host = services();
        let mut server = IpcServer::new(host.shared(), logger.clone());
        server.handle(
            "system.ping",
            move |services: Arc<Services>, input: SystemPingInput| {
                counted.fetch_add(1, Ordering::SeqCst);
                async move { Ok(services.system.ping(input.sent_at)) }
            },
        );
        for payload in [
            json!({ "sentAt": "not-a-number" }),
            json!({ "sentAt": -1 }),
            json!({}),
            Value::Null,
        ] {
            assert_eq!(
                server.dispatch("system.ping", payload).await,
                json!({
                    "ok": false,
                    "error": { "code": "VALIDATION_FAILED", "message": "Некорректные данные запроса" },
                })
            );
        }
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(logger.entries()[0].message, "Rejected invalid input");
        assert_eq!(logger.matching("Rejected invalid input").len(), 4);
    }

    #[tokio::test]
    async fn maps_an_app_error_to_its_code_and_keeps_details() {
        let (_host, mut server, logger) = server();
        native_failing(&mut server, || {
            AppError::new(AppErrorCode::NotFound, "Запись не найдена").with_detail("id", "7")
        });
        assert_eq!(
            server
                .dispatch("system.nativePing", json!({ "text": "hi" }))
                .await,
            json!({
                "ok": false,
                "error": { "code": "NOT_FOUND", "message": "Запись не найдена", "details": { "id": "7" } },
            })
        );
        assert_eq!(
            logger.matching("Handler failed")[0].fields["code"],
            json!("NOT_FOUND")
        );
    }

    #[tokio::test]
    async fn a_panicking_handler_costs_one_call_and_never_sends_the_detail() {
        let secret = "C:/host/only/secret-path.rs";
        let (_host, mut server, logger) = server();
        server.handle(
            "system.nativePing",
            move |_services: Arc<Services>, _input: SystemNativePingInput| async move {
                if secret.is_empty() {
                    return Ok(SystemNativePingOutput { count: 0 });
                }
                panic!("boom at {secret}")
            },
        );
        let result = server
            .dispatch("system.nativePing", json!({ "text": "hi" }))
            .await;
        assert_eq!(
            result,
            json!({ "ok": false, "error": { "code": "UNKNOWN", "message": "Внутренняя ошибка" } })
        );
        assert!(!result.to_string().contains("boom"));
        assert!(!result.to_string().contains(secret));
        let logged = logger.matching("Handler failed");
        assert!(
            logged[0].fields["error"]
                .as_str()
                .is_some_and(|error| error.contains("boom"))
        );
        assert_eq!(
            server
                .dispatch("system.ping", json!({ "sentAt": 1_000 }))
                .await["ok"],
            json!(true)
        );
    }

    #[tokio::test]
    async fn catches_output_that_drifts_from_the_contract_when_validation_is_on() {
        let drifting = |validate: bool| {
            let host = services();
            let logger = MemoryLogger::shared();
            let mut server =
                IpcServer::new(host.shared(), logger.clone()).with_output_validation(validate);
            server.handle(
                "system.ping",
                |_services: Arc<Services>, _input: SystemPingInput| async move {
                    Ok(SystemPingOutput {
                        pong: false,
                        host_time: 1,
                        round_trip_hint: 0.0,
                    })
                },
            );
            (host, server, logger)
        };
        let (_validating_host, validating, logger) = drifting(true);
        assert_eq!(
            validating
                .dispatch("system.ping", json!({ "sentAt": 1 }))
                .await,
            json!({ "ok": false, "error": { "code": "UNKNOWN", "message": "Внутренняя ошибка" } })
        );
        assert_eq!(
            logger
                .matching("Handler output does not match the contract")
                .len(),
            1
        );
        let (_trusting_host, trusting, _logger) = drifting(false);
        assert_eq!(
            trusting
                .dispatch("system.ping", json!({ "sentAt": 1 }))
                .await,
            json!({ "ok": true, "data": { "pong": false, "hostTime": 1, "roundTripHint": 0.0 } })
        );
    }

    #[tokio::test]
    async fn a_void_output_omits_data_and_a_void_input_accepts_nothing() {
        let logger = MemoryLogger::shared();
        let host = services();
        let mut server = IpcServer::new(host.shared(), logger);
        server.handle(
            "system.resources",
            |_services: Arc<Services>, _input: ()| async move { Ok(()) },
        );
        server.handle(
            "system.device",
            |_services: Arc<Services>, input: SystemDeviceInput| async move {
                Ok(input.refresh().to_string())
            },
        );
        assert_eq!(
            server.dispatch("system.resources", Value::Null).await,
            json!({ "ok": true })
        );
        assert_eq!(
            server.dispatch("system.device", json!({})).await,
            json!({ "ok": true, "data": "false" })
        );
    }

    #[tokio::test]
    async fn an_unregistered_channel_answers_not_implemented() {
        let (_host, server, logger) = server();
        for channel in ["providers.list", "system.demoStream", "unknown"] {
            let response = server.dispatch(channel, Value::Null).await;
            assert_eq!(response["ok"], json!(false));
            assert_eq!(response["error"]["code"], json!("NOT_IMPLEMENTED"));
            assert!(
                response["error"]["message"]
                    .as_str()
                    .is_some_and(|message| message.contains(channel))
            );
        }
        assert_eq!(logger.matching("not implemented").len(), 3);
    }

    #[test]
    fn finish_lists_every_missing_channel_in_one_line() {
        let (_host, server, logger) = server();
        let coverage = server.finish();
        assert_eq!(coverage.missing.len(), CHANNELS.len() - 1);
        assert!(!coverage.missing.contains(&"system.ping"));
        assert_eq!(server.channels(), ["system.ping"]);
        let lines = logger.entries();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].level, LogLevel::Warn);
        assert_eq!(lines[0].fields["missing"], json!(CHANNELS.len() - 1));
    }

    #[test]
    #[should_panic(expected = "is not a contract channel")]
    fn registering_a_channel_outside_the_contract_panics_at_startup() {
        let (_host, mut server, _logger) = server();
        server.handle(
            "system.nope",
            |_services: Arc<Services>, _input: ()| async move { Ok(()) },
        );
    }

    #[test]
    #[should_panic(expected = "registered twice")]
    fn registering_a_channel_twice_panics_at_startup() {
        let (_host, mut server, _logger) = server();
        server.handle(
            "system.ping",
            |_services: Arc<Services>, _input: ()| async move { Ok(()) },
        );
    }
}
