use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use serde_json::json;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    sync::mpsc::{UnboundedReceiver, unbounded_channel},
};
use zvs_core::CancellationToken;
use zvs_jobd::{
    download::{DOWNLOAD_JOB, part_path},
    jobs::{JobContext, dispatch},
    protocol::Response,
};

const BODY: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";

struct Server {
    url: String,
    ranges: Arc<AtomicUsize>,
}

/// A throwaway HTTP origin that honours `Range` unless told to ignore it. Enough of the
/// protocol for the resume path; no dependency on a web framework. `slow` trickles the body
/// out so a cancellation has something to interrupt.
async fn serve(ignore_range: bool, slow: bool) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let ranges = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&ranges);
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let counter = Arc::clone(&counter);
            tokio::spawn(async move {
                answer(stream, ignore_range, counter, slow).await;
            });
        }
    });
    Server {
        url: format!("http://127.0.0.1:{port}/blob"),
        ranges,
    }
}

async fn answer(stream: TcpStream, ignore_range: bool, ranges: Arc<AtomicUsize>, slow: bool) {
    let mut reader = BufReader::new(stream);
    let mut offset = 0_usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).await.unwrap_or(0) == 0 {
            return;
        }
        if line == "\r\n" {
            break;
        }
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("range: bytes=") {
            ranges.fetch_add(1, Ordering::SeqCst);
            if !ignore_range {
                offset = value
                    .split('-')
                    .next()
                    .and_then(|start| start.trim().parse().ok())
                    .unwrap_or(0);
            }
        }
    }
    let stream = reader.get_mut();
    let rest = &BODY[offset.min(BODY.len())..];
    let head = if offset > 0 {
        format!(
            "HTTP/1.1 206 Partial Content\r\nAccept-Ranges: bytes\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\nConnection: close\r\n\r\n",
            rest.len(),
            offset,
            BODY.len() - 1,
            BODY.len()
        )
    } else {
        format!(
            "HTTP/1.1 200 OK\r\nAccept-Ranges: bytes\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            rest.len()
        )
    };
    if stream.write_all(head.as_bytes()).await.is_err() {
        return;
    }
    // A stalled body gives a cancellation test something to interrupt mid-transfer.
    for chunk in rest.chunks(8) {
        if stream.write_all(chunk).await.is_err() || stream.flush().await.is_err() {
            return;
        }
        if slow {
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }
}

fn context(token: CancellationToken) -> (JobContext, UnboundedReceiver<Response>) {
    let (sender, receiver) = unbounded_channel();
    (
        JobContext::new("d", sender, token, Duration::ZERO),
        receiver,
    )
}

fn sha256(bytes: &[u8]) -> String {
    use sha2::Digest as _;
    sha2::Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[tokio::test]
async fn a_download_verifies_its_checksum_and_lands_at_the_target() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("model.bin");
    let server = serve(false, false).await;
    let (context, mut progress) = context(CancellationToken::new());

    let result = dispatch(
        DOWNLOAD_JOB,
        json!({
            "url": server.url,
            "targetPath": target.to_string_lossy(),
            "checksum": { "algorithm": "sha256", "value": sha256(BODY) },
        }),
        &context,
    )
    .await
    .unwrap();

    assert_eq!(result["bytes"], BODY.len());
    assert_eq!(result["resumedFrom"], 0);
    assert_eq!(result["resumable"], true);
    assert_eq!(std::fs::read(&target).unwrap(), BODY);
    assert!(!part_path(&target).exists());

    let mut seen = Vec::new();
    while let Ok(Response::JobProgress { done, total, .. }) = progress.try_recv() {
        seen.push((done, total));
    }
    assert_eq!(seen.first(), Some(&(0, BODY.len() as u64)));
    assert_eq!(seen.last(), Some(&(BODY.len() as u64, BODY.len() as u64)));
}

#[tokio::test]
async fn a_partial_file_resumes_with_a_range_request_instead_of_restarting() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("model.bin");
    std::fs::write(part_path(&target), &BODY[..10]).unwrap();
    let server = serve(false, false).await;
    let (context, _progress) = context(CancellationToken::new());

    let result = dispatch(
        DOWNLOAD_JOB,
        json!({ "url": server.url, "targetPath": target.to_string_lossy() }),
        &context,
    )
    .await
    .unwrap();

    assert_eq!(result["resumedFrom"], 10);
    assert_eq!(server.ranges.load(Ordering::SeqCst), 1);
    assert_eq!(std::fs::read(&target).unwrap(), BODY);
}

#[tokio::test]
async fn a_server_that_ignores_the_range_restarts_from_zero_without_corrupting_the_file() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("model.bin");
    std::fs::write(part_path(&target), &BODY[..10]).unwrap();
    let server = serve(true, false).await;
    let (context, _progress) = context(CancellationToken::new());

    let result = dispatch(
        DOWNLOAD_JOB,
        json!({ "url": server.url, "targetPath": target.to_string_lossy() }),
        &context,
    )
    .await
    .unwrap();

    assert_eq!(result["resumedFrom"], 0);
    assert_eq!(std::fs::read(&target).unwrap(), BODY);
}

#[tokio::test]
async fn a_checksum_mismatch_fails_and_installs_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("model.bin");
    let server = serve(false, false).await;
    let (context, _progress) = context(CancellationToken::new());

    let error = dispatch(
        DOWNLOAD_JOB,
        json!({
            "url": server.url,
            "targetPath": target.to_string_lossy(),
            "checksum": { "algorithm": "sha256", "value": sha256(b"something else") },
        }),
        &context,
    )
    .await
    .unwrap_err();

    assert_eq!(error.code(), "VALIDATION_FAILED");
    assert!(!target.exists(), "a corrupt download must never land");
    assert!(
        !part_path(&target).exists(),
        "a corrupt partial must not be resumed from"
    );
}

#[tokio::test]
async fn cancelling_keeps_the_partial_file_for_the_next_attempt() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("model.bin");
    let server = serve(false, true).await;
    let token = CancellationToken::new();
    let (context, _progress) = context(token.clone());

    let cancelling = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(250)).await;
        token.cancel();
    });
    let error = dispatch(
        DOWNLOAD_JOB,
        json!({ "url": server.url, "targetPath": target.to_string_lossy() }),
        &context,
    )
    .await
    .unwrap_err();
    cancelling.await.unwrap();

    assert_eq!(error.code(), "RUN_CANCELLED");
    assert!(!target.exists());
    let partial = std::fs::metadata(part_path(&target)).unwrap().len();
    assert!(
        partial > 0 && partial < BODY.len() as u64,
        "kept {partial} bytes"
    );
}

#[tokio::test]
async fn only_http_urls_and_real_targets_are_accepted() {
    let directory = tempfile::tempdir().unwrap();
    let (context, _progress) = context(CancellationToken::new());
    for params in [
        json!({ "url": "file:///etc/passwd", "targetPath": directory.path().join("a").to_string_lossy() }),
        json!({ "url": "http://127.0.0.1:1/x" }),
        json!({ "url": "http://127.0.0.1:1/x", "targetPath": "a", "nope": 1 }),
    ] {
        let error = dispatch(DOWNLOAD_JOB, params, &context).await.unwrap_err();
        assert_eq!(error.code(), "VALIDATION_FAILED");
    }
}
