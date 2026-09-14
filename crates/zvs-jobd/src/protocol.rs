use serde::{Deserialize, Serialize};

pub const UNMATCHED: &str = "";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type")]
pub enum Request {
    #[serde(rename = "ping")]
    Ping { id: String },
    #[serde(rename = "job.start")]
    JobStart {
        id: String,
        job: String,
        #[serde(default)]
        params: serde_json::Value,
    },
    #[serde(rename = "job.cancel")]
    JobCancel { id: String },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type")]
pub enum Response {
    #[serde(rename = "ping")]
    Ping { id: String },
    #[serde(rename = "job.progress")]
    JobProgress {
        id: String,
        done: u64,
        total: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    #[serde(rename = "job.done")]
    JobDone {
        id: String,
        result: serde_json::Value,
    },
    #[serde(rename = "job.error")]
    JobError {
        id: String,
        code: String,
        message: String,
    },
}

impl Response {
    pub fn id(&self) -> &str {
        match self {
            Self::Ping { id }
            | Self::JobProgress { id, .. }
            | Self::JobDone { id, .. }
            | Self::JobError { id, .. } => id,
        }
    }

    pub fn failure(id: impl Into<String>, error: &zvs_core::Error) -> Self {
        Self::JobError {
            id: id.into(),
            code: error.code().to_owned(),
            message: error.to_string(),
        }
    }

    pub fn refusal(id: impl Into<String>, code: &str, message: impl Into<String>) -> Self {
        Self::JobError {
            id: id.into(),
            code: code.to_owned(),
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_use_the_documented_wire_names() {
        let start: Request =
            serde_json::from_str(r#"{"type":"job.start","id":"a","job":"job.sleep"}"#).unwrap();
        assert_eq!(
            start,
            Request::JobStart {
                id: "a".into(),
                job: "job.sleep".into(),
                params: serde_json::Value::Null,
            }
        );
        assert_eq!(
            serde_json::from_str::<Request>(r#"{"type":"job.cancel","id":"a"}"#).unwrap(),
            Request::JobCancel { id: "a".into() }
        );
        assert!(serde_json::from_str::<Request>(r#"{"type":"job.pause","id":"a"}"#).is_err());
    }

    #[test]
    fn responses_carry_their_request_id_and_omit_absent_progress_messages() {
        let progress = Response::JobProgress {
            id: "a".into(),
            done: 1,
            total: 2,
            message: None,
        };
        assert_eq!(progress.id(), "a");
        assert_eq!(
            serde_json::to_string(&progress).unwrap(),
            r#"{"type":"job.progress","id":"a","done":1,"total":2}"#
        );
        assert_eq!(
            Response::failure("a", &zvs_core::Error::Cancelled),
            Response::JobError {
                id: "a".into(),
                code: "RUN_CANCELLED".into(),
                message: "Operation cancelled".into(),
            }
        );
    }
}
