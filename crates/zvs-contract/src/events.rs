use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::errors::{AppError, AppErrorCode};
use crate::ids::StreamId;
use crate::primitives::Timestamp;
use crate::questions::QuestionRequestDto;
use crate::validate::{Validate, check, nested, no_refinements};

pub const EVENT_CHANNEL: &str = "zvs:events";

pub type EventSeq = u64;

pub type StepEventDto = Map<String, Value>;

pub type LogLineDto = Map<String, Value>;

pub type ApprovalRequestDto = Map<String, Value>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeltaKind {
    Text,
    Reasoning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunOutcomeStatus {
    Ok,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunOutcomeDto {
    pub status: RunOutcomeStatus,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub code: Option<AppErrorCode>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum HostEventDraft {
    Token {
        delta: String,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "crate::wire::present"
        )]
        kind: Option<DeltaKind>,
    },
    Progress {
        done: u64,
        total: u64,
    },
    Step {
        step: StepEventDto,
    },
    Log {
        line: LogLineDto,
    },
    Approval {
        request: ApprovalRequestDto,
    },
    Question {
        request: QuestionRequestDto,
    },
    End {
        outcome: RunOutcomeDto,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostEvent {
    pub stream_id: StreamId,
    pub seq: EventSeq,
    pub ts: Timestamp,
    #[serde(flatten)]
    pub body: HostEventDraft,
}

impl HostEvent {
    pub const fn new(
        stream_id: StreamId,
        seq: EventSeq,
        ts: Timestamp,
        body: HostEventDraft,
    ) -> Self {
        Self {
            stream_id,
            seq,
            ts,
            body,
        }
    }
}

impl Validate for HostEventDraft {
    fn validate(&self) -> Result<(), AppError> {
        match self {
            Self::Progress { done, total } => {
                check::safe_integer("done", *done)?;
                check::safe_integer("total", *total)
            }
            Self::Question { request } => nested("request", request.validate()),
            _ => Ok(()),
        }
    }
}

impl Validate for HostEvent {
    fn validate(&self) -> Result<(), AppError> {
        check::safe_integer("seq", self.seq)?;
        check::timestamp("ts", self.ts)?;
        self.body.validate()
    }
}

no_refinements!(DeltaKind, RunOutcomeStatus, RunOutcomeDto);

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const STREAM: &str = "0199aa11-1111-7111-8111-111111111111";

    fn event(body: HostEventDraft) -> HostEvent {
        HostEvent::new(
            StreamId::parse(STREAM).expect("uuid"),
            3,
            1_700_000_000_000,
            body,
        )
    }

    #[test]
    fn the_event_channel_matches_the_renderer() {
        assert_eq!(EVENT_CHANNEL, "zvs:events");
    }

    #[test]
    fn a_token_event_flattens_the_envelope_and_tags_the_type() {
        let token = event(HostEventDraft::Token {
            delta: "пр".to_owned(),
            kind: Some(DeltaKind::Reasoning),
        });
        assert_eq!(
            serde_json::to_value(&token).expect("serialises"),
            json!({
                "streamId": STREAM,
                "seq": 3,
                "ts": 1_700_000_000_000_i64,
                "type": "token",
                "delta": "пр",
                "kind": "reasoning",
            })
        );
    }

    #[test]
    fn an_end_event_omits_absent_outcome_fields() {
        let end = event(HostEventDraft::End {
            outcome: RunOutcomeDto {
                status: RunOutcomeStatus::Ok,
                code: None,
                message: None,
            },
        });
        assert_eq!(
            serde_json::to_value(&end).expect("serialises"),
            json!({
                "streamId": STREAM,
                "seq": 3,
                "ts": 1_700_000_000_000_i64,
                "type": "end",
                "outcome": { "status": "ok" },
            })
        );
    }

    #[test]
    fn an_unknown_type_is_rejected() {
        let raw = json!({ "streamId": STREAM, "seq": 0, "ts": 0, "type": "nope" });
        assert!(serde_json::from_value::<HostEvent>(raw).is_err());
    }

    #[test]
    fn progress_round_trips() {
        let raw = json!({ "streamId": STREAM, "seq": 0, "ts": 0, "type": "progress", "done": 3, "total": 10 });
        let parsed: HostEvent = serde_json::from_value(raw.clone()).expect("parses");
        assert_eq!(serde_json::to_value(&parsed).expect("serialises"), raw);
    }
}
