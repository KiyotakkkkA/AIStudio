use std::time::{Duration, Instant};

use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, Lines, duplex},
    task::JoinHandle,
    time::timeout,
};
use zvs_jobd::{
    protocol::{Request, Response},
    server::serve,
};

const LIMIT: Duration = Duration::from_secs(10);

struct Sidecar {
    input: DuplexStream,
    output: Lines<BufReader<DuplexStream>>,
    process: JoinHandle<std::io::Result<()>>,
}

impl Sidecar {
    fn start() -> Self {
        let (input, reads) = duplex(64 * 1024);
        let (writes, output) = duplex(64 * 1024);
        Self {
            input,
            output: BufReader::new(output).lines(),
            process: tokio::spawn(serve(reads, writes)),
        }
    }

    async fn send(&mut self, request: &Request) {
        let line = format!("{}\n", serde_json::to_string(request).unwrap());
        self.input.write_all(line.as_bytes()).await.unwrap();
        self.input.flush().await.unwrap();
    }

    async fn raw(&mut self, line: &str) {
        self.input
            .write_all(format!("{line}\n").as_bytes())
            .await
            .unwrap();
        self.input.flush().await.unwrap();
    }

    async fn next(&mut self) -> Response {
        let line = timeout(LIMIT, self.output.next_line())
            .await
            .expect("the sidecar answered within the time limit")
            .unwrap()
            .expect("the sidecar is still writing");
        serde_json::from_str(&line).unwrap()
    }

    async fn settle(&mut self, id: &str) -> Response {
        loop {
            let response = self.next().await;
            if response.id() == id && !matches!(response, Response::JobProgress { .. }) {
                return response;
            }
        }
    }

    async fn stop(mut self) {
        self.input.shutdown().await.unwrap();
        timeout(LIMIT, self.process)
            .await
            .expect("the sidecar exited after end of file")
            .unwrap()
            .unwrap();
    }
}

fn sleep(id: &str, steps: u64, interval_ms: u64) -> Request {
    Request::JobStart {
        id: id.into(),
        job: "job.sleep".into(),
        params: serde_json::json!({ "steps": steps, "intervalMs": interval_ms }),
    }
}

#[tokio::test]
async fn answers_ping_and_matches_concurrent_jobs_to_their_request_ids() {
    let mut sidecar = Sidecar::start();
    sidecar
        .send(&Request::Ping {
            id: "health".into(),
        })
        .await;
    assert_eq!(
        sidecar.next().await,
        Response::Ping {
            id: "health".into()
        }
    );

    sidecar.send(&sleep("slow", 4, 30)).await;
    sidecar.send(&sleep("fast", 1, 0)).await;
    let mut progress = 0;
    let mut finished = Vec::new();
    while finished.len() < 2 {
        match sidecar.next().await {
            Response::JobProgress {
                id, done, total, ..
            } => {
                assert!(done <= total);
                if id == "slow" {
                    progress += 1;
                }
            }
            Response::JobDone { id, result } => {
                let expected = if id == "slow" { 4 } else { 1 };
                assert_eq!(result, serde_json::json!({ "steps": expected }));
                finished.push(id);
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }
    assert_eq!(finished, ["fast", "slow"]);
    assert!(progress >= 2, "the slow job reported progress as it ran");
    sidecar.stop().await;
}

#[tokio::test]
async fn cancel_stops_a_running_job_promptly() {
    let mut sidecar = Sidecar::start();
    sidecar.send(&sleep("long", 10_000, 5)).await;
    assert!(matches!(
        sidecar.next().await,
        Response::JobProgress { done: 0, .. }
    ));
    let at = Instant::now();
    sidecar
        .send(&Request::JobCancel { id: "long".into() })
        .await;
    let Response::JobError { id, code, .. } = sidecar.settle("long").await else {
        panic!("a cancelled job ends with job.error");
    };
    assert_eq!((id.as_str(), code.as_str()), ("long", "RUN_CANCELLED"));
    assert!(at.elapsed() < Duration::from_secs(5));
    sidecar.stop().await;
}

#[tokio::test]
async fn refuses_unreadable_lines_unknown_jobs_and_duplicate_ids_without_stopping() {
    let mut sidecar = Sidecar::start();
    sidecar.raw("not json at all").await;
    let Response::JobError { id, code, .. } = sidecar.next().await else {
        panic!("an unreadable line is refused");
    };
    assert_eq!((id.as_str(), code.as_str()), ("", "VALIDATION_FAILED"));

    sidecar
        .send(&Request::JobStart {
            id: "unknown".into(),
            job: "job.nope".into(),
            params: serde_json::Value::Null,
        })
        .await;
    let Response::JobError { id, code, message } = sidecar.settle("unknown").await else {
        panic!("an unknown job is refused");
    };
    assert_eq!(
        (id.as_str(), code.as_str()),
        ("unknown", "VALIDATION_FAILED")
    );
    assert!(message.contains("job.nope"));

    sidecar.send(&sleep("twice", 200, 20)).await;
    sidecar.send(&sleep("twice", 1, 0)).await;
    let Response::JobError { code, .. } = sidecar.settle("twice").await else {
        panic!("a duplicate id is refused");
    };
    assert_eq!(code, "CONFLICT");

    sidecar
        .send(&Request::Ping {
            id: "still here".into(),
        })
        .await;
    assert_eq!(
        sidecar.settle("still here").await,
        Response::Ping {
            id: "still here".into()
        }
    );
    sidecar.stop().await;
}

#[tokio::test]
async fn end_of_input_cancels_jobs_still_in_flight() {
    let mut sidecar = Sidecar::start();
    sidecar.send(&sleep("orphan", 10_000, 5)).await;
    assert!(matches!(
        sidecar.next().await,
        Response::JobProgress { done: 0, .. }
    ));
    sidecar.input.shutdown().await.unwrap();
    let Response::JobError { code, .. } = sidecar.settle("orphan").await else {
        panic!("an orphaned job ends with job.error");
    };
    assert_eq!(code, "RUN_CANCELLED");
    timeout(LIMIT, sidecar.process)
        .await
        .expect("the sidecar exited after end of file")
        .unwrap()
        .unwrap();
}
