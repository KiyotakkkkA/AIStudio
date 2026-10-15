use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type")]
pub enum SidecarRequest<'a> {
    #[serde(rename = "ping")]
    Ping { id: &'a str },
    #[serde(rename = "job.start")]
    JobStart {
        id: &'a str,
        job: &'a str,
        params: &'a Value,
    },
    #[serde(rename = "job.cancel")]
    JobCancel { id: &'a str },
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type")]
pub enum SidecarResponse {
    #[serde(rename = "ping")]
    Ping { id: String },
    #[serde(rename = "job.progress")]
    JobProgress {
        id: String,
        done: u64,
        total: u64,
        #[serde(default)]
        rate: Option<f64>,
        #[serde(default)]
        message: Option<String>,
    },
    #[serde(rename = "job.done")]
    JobDone { id: String, result: Value },
    #[serde(rename = "job.error")]
    JobError {
        id: String,
        code: String,
        message: String,
    },
}

impl SidecarResponse {
    pub fn parse(line: &str) -> Result<Self, String> {
        let response: Self = serde_json::from_str(line).map_err(|error| error.to_string())?;
        if let Self::JobProgress {
            rate: Some(rate), ..
        } = &response
            && (!rate.is_finite() || *rate < 0.0)
        {
            return Err("rate must be a non-negative number".to_owned());
        }
        Ok(response)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct JobProgress {
    pub done: u64,
    pub total: u64,
    pub rate: Option<f64>,
    pub message: Option<String>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn requests_use_the_documented_wire_names() {
        let params = json!({ "steps": 2 });
        assert_eq!(
            serde_json::to_value(SidecarRequest::JobStart {
                id: "a",
                job: "job.sleep",
                params: &params,
            })
            .expect("serialises"),
            json!({ "type": "job.start", "id": "a", "job": "job.sleep", "params": { "steps": 2 } })
        );
        assert_eq!(
            serde_json::to_value(SidecarRequest::JobCancel { id: "a" }).expect("serialises"),
            json!({ "type": "job.cancel", "id": "a" })
        );
        assert_eq!(
            serde_json::to_value(SidecarRequest::Ping { id: "p" }).expect("serialises"),
            json!({ "type": "ping", "id": "p" })
        );
    }

    #[test]
    fn responses_are_parsed_and_malformed_ones_refused() {
        assert_eq!(
            SidecarResponse::parse(
                r#"{"type":"job.progress","id":"a","done":1,"total":2,"rate":3.5,"message":"m"}"#
            ),
            Ok(SidecarResponse::JobProgress {
                id: "a".to_owned(),
                done: 1,
                total: 2,
                rate: Some(3.5),
                message: Some("m".to_owned()),
            })
        );
        assert_eq!(
            SidecarResponse::parse(r#"{"type":"job.done","id":"a","result":null}"#),
            Ok(SidecarResponse::JobDone {
                id: "a".to_owned(),
                result: Value::Null,
            })
        );
        for bad in [
            "not json",
            r#"{"type":"job.pause","id":"a"}"#,
            r#"{"type":"job.progress","id":"a","done":-1,"total":2}"#,
            r#"{"type":"job.progress","id":"a","done":1.5,"total":2}"#,
            r#"{"type":"job.progress","id":"a","done":1,"total":2,"rate":-1}"#,
            r#"{"type":"job.error","id":"a","code":"X"}"#,
            r#"{"type":"job.done","id":"a"}"#,
        ] {
            assert!(SidecarResponse::parse(bad).is_err(), "{bad}");
        }
    }
}
