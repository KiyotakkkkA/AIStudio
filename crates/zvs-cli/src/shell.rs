use std::process::Stdio;
use std::sync::{Arc, Mutex, PoisonError};

use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot};

use crate::state::transcript::ShellEnd;

#[derive(Clone, Default)]
pub struct ShellControl {
    stop: Arc<Mutex<Option<oneshot::Sender<()>>>>,
}

impl ShellControl {
    pub fn stop(&self) {
        if let Some(sender) = self
            .stop
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
            let _ = sender.send(());
        }
    }

    fn arm(&self) -> oneshot::Receiver<()> {
        let (sender, receiver) = oneshot::channel();
        *self.stop.lock().unwrap_or_else(PoisonError::into_inner) = Some(sender);
        receiver
    }
}

#[cfg(windows)]
pub fn command(line: &str, folder: &str) -> Command {
    let mut command = Command::new("cmd.exe");
    command.raw_arg(format!("/D /S /C \"{line}\""));
    command.current_dir(folder);
    command
}

#[cfg(not(windows))]
pub fn command(line: &str, folder: &str) -> Command {
    let mut command = Command::new("sh");
    command.arg("-c").arg(line).current_dir(folder);
    command
}

pub fn decode(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_owned(),
        #[cfg(windows)]
        Err(_) => zvs_pipe::oem_text(bytes),
        #[cfg(not(windows))]
        Err(_) => String::from_utf8_lossy(bytes).into_owned(),
    }
}

fn pump<R: AsyncRead + Unpin + Send + 'static>(reader: R, lines: mpsc::UnboundedSender<String>) {
    tokio::spawn(async move {
        let mut reader = BufReader::new(reader);
        let mut buffer = Vec::new();
        loop {
            buffer.clear();
            match reader.read_until(b'\n', &mut buffer).await {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let line = decode(&buffer).trim_end_matches(['\r', '\n']).to_owned();
                    if lines.send(line).is_err() {
                        break;
                    }
                }
            }
        }
    });
}

#[cfg(windows)]
async fn kill_tree(child: &mut Child) {
    if let Some(pid) = child.id() {
        let _ = Command::new("taskkill")
            .args(["/T", "/F", "/PID", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
    }
    let _ = child.kill().await;
}

#[cfg(not(windows))]
async fn kill_tree(child: &mut Child) {
    let _ = child.kill().await;
}

pub async fn run(
    line: String,
    folder: String,
    control: ShellControl,
    output: impl Fn(String) + Send + 'static,
) -> ShellEnd {
    let stop = control.arm();
    let spawned = command(&line, &folder)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn();
    let Ok(mut child) = spawned else {
        return ShellEnd::Failed;
    };
    let (lines, mut received) = mpsc::unbounded_channel();
    if let Some(stdout) = child.stdout.take() {
        pump(stdout, lines.clone());
    }
    if let Some(stderr) = child.stderr.take() {
        pump(stderr, lines.clone());
    }
    drop(lines);
    tokio::pin!(stop);
    let mut stopped = false;
    let status = loop {
        tokio::select! {
            line = received.recv() => match line {
                Some(line) => output(line),
                None => break child.wait().await,
            },
            _ = &mut stop, if !stopped => {
                stopped = true;
                kill_tree(&mut child).await;
            }
        }
    };
    while let Ok(line) = received.try_recv() {
        output(line);
    }
    if stopped {
        return ShellEnd::Stopped;
    }
    match status {
        Ok(status) => ShellEnd::Exited(status.code().unwrap_or(-1)),
        Err(_) => ShellEnd::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_command_streams_its_output_and_reports_the_exit_code() {
        let folder = std::env::temp_dir().to_string_lossy().into_owned();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        let end = run(
            "echo привет&& exit 3".to_owned(),
            folder,
            ShellControl::default(),
            move |line| sink.lock().expect("lines").push(line),
        )
        .await;
        assert_eq!(end, ShellEnd::Exited(3));
        assert_eq!(*seen.lock().expect("lines"), ["привет"]);
    }

    #[tokio::test]
    async fn a_stopped_command_ends_as_stopped() {
        let folder = std::env::temp_dir().to_string_lossy().into_owned();
        let control = ShellControl::default();
        let stopper = control.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            stopper.stop();
        });
        let end = run("ping -n 30 127.0.0.1".to_owned(), folder, control, |_| {}).await;
        assert_eq!(end, ShellEnd::Stopped);
    }
}
