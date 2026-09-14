use std::{
    collections::HashMap,
    io,
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};

use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader},
    sync::mpsc::{UnboundedSender, unbounded_channel},
};
use zvs_core::CancellationToken;

use crate::{
    jobs::{JobContext, dispatch},
    protocol::{Request, Response, UNMATCHED},
};

pub const PROGRESS_INTERVAL: Duration = Duration::from_millis(50);

type Jobs = Arc<Mutex<HashMap<String, CancellationToken>>>;

fn held<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Reads newline-delimited JSON requests, dispatches each to a job task, and writes
/// newline-delimited JSON responses. Returns once the input reaches end of file and every
/// job it started has finished.
pub async fn serve<R, W>(input: R, output: W) -> io::Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let (sender, mut responses) = unbounded_channel::<Response>();
    let jobs: Jobs = Jobs::default();

    let reading = {
        let jobs = Arc::clone(&jobs);
        let sender = sender.clone();
        async move {
            let mut lines = BufReader::new(input).lines();
            while let Some(line) = lines.next_line().await? {
                if line.trim().is_empty() {
                    continue;
                }
                accept(&line, &jobs, &sender);
            }
            for token in held(&jobs).values() {
                token.cancel();
            }
            io::Result::Ok(())
        }
    };
    drop(sender);

    let writing = async move {
        let mut output = output;
        while let Some(response) = responses.recv().await {
            let mut line = serde_json::to_string(&response).map_err(io::Error::other)?;
            line.push('\n');
            output.write_all(line.as_bytes()).await?;
            output.flush().await?;
        }
        io::Result::Ok(())
    };

    let (read, written) = tokio::join!(reading, writing);
    read.and(written)
}

fn accept(line: &str, jobs: &Jobs, sender: &UnboundedSender<Response>) {
    let request = match serde_json::from_str::<Request>(line) {
        Ok(request) => request,
        Err(error) => {
            let _ = sender.send(Response::refusal(
                UNMATCHED,
                "VALIDATION_FAILED",
                format!("Unreadable request: {error}"),
            ));
            return;
        }
    };
    match request {
        Request::Ping { id } => {
            let _ = sender.send(Response::Ping { id });
        }
        Request::JobCancel { id } => {
            if let Some(token) = held(jobs).get(&id) {
                token.cancel();
            } else {
                eprintln!("zvs-jobd: cancel for an unknown job {id}");
            }
        }
        Request::JobStart { id, job, params } => start(id, job, params, jobs, sender),
    }
}

fn start(
    id: String,
    job: String,
    params: serde_json::Value,
    jobs: &Jobs,
    sender: &UnboundedSender<Response>,
) {
    let token = CancellationToken::new();
    if held(jobs).insert(id.clone(), token.clone()).is_some() {
        let _ = sender.send(Response::refusal(
            id,
            "CONFLICT",
            "A job with this id is already running",
        ));
        return;
    }
    let context = JobContext::new(id.clone(), sender.clone(), token, PROGRESS_INTERVAL);
    let sender = sender.clone();
    let jobs = Arc::clone(jobs);
    tokio::spawn(async move {
        let response = match dispatch(&job, params, &context).await {
            Ok(result) => Response::JobDone {
                id: id.clone(),
                result,
            },
            Err(error) => Response::failure(id.clone(), &error),
        };
        held(&jobs).remove(&id);
        let _ = sender.send(response);
    });
}
