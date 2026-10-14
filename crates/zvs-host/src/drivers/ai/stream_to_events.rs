use futures::StreamExt;
use zvs_contract::{
    AppError, AppErrorCode, DeltaKind, HostEventDraft, RunOutcomeDto, RunOutcomeStatus,
};

use super::errors::is_cancellation;
use super::ports::{DeltaStream, TextDelta};
use crate::platform::events::StreamHandle;

pub const CANCELLED_OUTCOME_MESSAGE: &str = "Отменено";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamToEventsResult {
    pub text: String,
    pub reasoning: String,
    pub outcome: RunOutcomeDto,
}

pub fn ok_outcome() -> RunOutcomeDto {
    RunOutcomeDto {
        status: RunOutcomeStatus::Ok,
        code: None,
        message: None,
    }
}

pub fn outcome_of(error: &AppError) -> RunOutcomeDto {
    if is_cancellation(error) {
        return RunOutcomeDto {
            status: RunOutcomeStatus::Cancelled,
            code: Some(AppErrorCode::RunCancelled),
            message: Some(CANCELLED_OUTCOME_MESSAGE.to_owned()),
        };
    }
    RunOutcomeDto {
        status: RunOutcomeStatus::Failed,
        code: Some(error.code),
        message: Some(error.message.clone()),
    }
}

pub async fn stream_to_events(
    stream: &StreamHandle,
    mut deltas: DeltaStream,
) -> StreamToEventsResult {
    let mut text = String::new();
    let mut reasoning = String::new();
    let mut outcome = ok_outcome();
    while let Some(delta) = deltas.next().await {
        match delta {
            Ok(TextDelta::Content {
                text: delta,
                kind: DeltaKind::Reasoning,
            }) if !delta.is_empty() => {
                reasoning.push_str(&delta);
                stream.emit(HostEventDraft::Token {
                    delta,
                    kind: Some(DeltaKind::Reasoning),
                });
            }
            Ok(TextDelta::Content {
                text: delta,
                kind: DeltaKind::Text,
            }) if !delta.is_empty() => {
                text.push_str(&delta);
                stream.emit(HostEventDraft::Token { delta, kind: None });
            }
            Ok(_) => {}
            Err(error) => {
                outcome = outcome_of(&error);
                break;
            }
        }
    }
    drop(deltas);
    stream.end(outcome.clone());
    StreamToEventsResult {
        text,
        reasoning,
        outcome,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::drivers::ai::mock::FakeTextDriver;
    use crate::drivers::ai::ports::{ChatMessage, GenerateRequest, TextGenerationDriver};
    use crate::platform::events::{EventBus, EventBusOptions};
    use crate::testing::MemorySink;
    use tokio_util::sync::CancellationToken;
    use zvs_contract::HostEvent;

    fn request() -> GenerateRequest {
        GenerateRequest::new("llama3", vec![ChatMessage::user("Привет")])
    }

    fn attached_bus() -> (EventBus, Arc<MemorySink>) {
        let bus = EventBus::new(EventBusOptions::default());
        let sink = MemorySink::shared();
        bus.attach(sink.clone());
        (bus, sink)
    }

    fn tokens(events: &[HostEvent]) -> Vec<String> {
        events
            .iter()
            .filter_map(|event| match &event.body {
                HostEventDraft::Token { delta, .. } => Some(delta.clone()),
                _ => None,
            })
            .collect()
    }

    fn last_outcome(events: &[HostEvent]) -> Option<RunOutcomeDto> {
        match events.last().map(|event| &event.body) {
            Some(HostEventDraft::End { outcome }) => Some(outcome.clone()),
            _ => None,
        }
    }

    #[tokio::test]
    async fn streaming_re_emits_deltas_as_token_events_and_closes_with_the_outcome() {
        let (bus, sink) = attached_bus();
        let driver = FakeTextDriver::with_deltas(&["При", "вет"]);
        let stream = bus.open_stream().expect("a stream");
        let result = stream_to_events(
            &stream,
            driver.stream(&request(), &CancellationToken::new()),
        )
        .await;
        assert_eq!(result.text, "Привет");
        assert_eq!(tokens(&sink.events()), ["При", "вет"]);
        assert_eq!(last_outcome(&sink.events()), Some(ok_outcome()));
        assert!(stream.closed());

        let (bus, sink) = attached_bus();
        let failing = FakeTextDriver::with_deltas(&["a", "b"])
            .failing_after(1, AppError::new(AppErrorCode::RateLimited, "слишком часто"));
        let result = stream_to_events(
            &bus.open_stream().expect("a stream"),
            failing.stream(&request(), &CancellationToken::new()),
        )
        .await;
        assert_eq!(
            result.outcome,
            RunOutcomeDto {
                status: RunOutcomeStatus::Failed,
                code: Some(AppErrorCode::RateLimited),
                message: Some("слишком часто".to_owned()),
            }
        );
        assert_eq!(result.text, "a");
        assert_eq!(tokens(&sink.events()), ["a"]);
    }

    #[tokio::test]
    async fn aborting_mid_stream_ends_the_stream_as_cancelled() {
        let (bus, sink) = attached_bus();
        let driver =
            Arc::new(FakeTextDriver::with_deltas(&["one", "two", "three"]).holding_after(1));
        let cancel = CancellationToken::new();
        let stream = bus.open_stream().expect("a stream");
        let deltas = driver.stream(&request(), &cancel);
        let pump = tokio::spawn(async move { stream_to_events(&stream, deltas).await });
        driver.when_holding().await;
        cancel.cancel();
        let result = pump.await.expect("joins");
        assert_eq!(result.outcome.status, RunOutcomeStatus::Cancelled);
        assert_eq!(result.outcome.code, Some(AppErrorCode::RunCancelled));
        assert_eq!(
            result.outcome.message.as_deref(),
            Some(CANCELLED_OUTCOME_MESSAGE)
        );
        assert_eq!(driver.emitted(), 1);
        assert_eq!(result.text, "one");
        assert_eq!(tokens(&sink.events()), ["one"]);
        assert!(matches!(
            sink.events().last().map(|event| &event.body),
            Some(HostEventDraft::End { .. })
        ));
        assert_eq!(bus.open(), 0);
    }

    #[tokio::test]
    async fn reasoning_is_emitted_with_its_kind_and_empty_or_tool_deltas_are_skipped() {
        let (bus, sink) = attached_bus();
        let deltas = futures::stream::iter([
            Ok(TextDelta::reasoning("think")),
            Ok(TextDelta::text("")),
            Ok(TextDelta::ToolCall(
                crate::drivers::ai::ports::ToolCall::new("c", "n", "{}"),
            )),
            Ok(TextDelta::text("say")),
        ])
        .boxed();
        let result = stream_to_events(&bus.open_stream().expect("a stream"), deltas).await;
        assert_eq!(result.reasoning, "think");
        assert_eq!(result.text, "say");
        let events = sink.events();
        assert_eq!(
            events[0].body,
            HostEventDraft::Token {
                delta: "think".to_owned(),
                kind: Some(DeltaKind::Reasoning)
            }
        );
        assert_eq!(
            events[1].body,
            HostEventDraft::Token {
                delta: "say".to_owned(),
                kind: None
            }
        );
        assert_eq!(events.len(), 3);
    }
}
