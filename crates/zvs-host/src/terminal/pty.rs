use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PtySpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
    pub cols: u16,
    pub rows: u16,
}

pub struct PtyParts {
    pub reader: Box<dyn Read + Send>,
    pub writer: Box<dyn Write + Send>,
    pub control: Box<dyn PtyControl>,
    pub child: Box<dyn PtyChild>,
}

pub trait PtyControl: Send + Sync {
    fn resize(&self, cols: u16, rows: u16) -> io::Result<()>;
    fn kill(&self);
    fn release(&self);
}

pub trait PtyChild: Send {
    fn wait(&mut self) -> Option<i32>;
}

pub trait PtySpawner: Send + Sync {
    fn spawn(&self, spec: &PtySpec) -> io::Result<PtyParts>;
}

fn io_error(error: impl std::fmt::Display) -> io::Error {
    io::Error::other(error.to_string())
}

const fn size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    }
}

pub struct NativePty;

impl PtySpawner for NativePty {
    fn spawn(&self, spec: &PtySpec) -> io::Result<PtyParts> {
        let pair = native_pty_system()
            .openpty(size(spec.cols, spec.rows))
            .map_err(io_error)?;
        let mut command = CommandBuilder::new(&spec.program);
        command.args(&spec.args);
        command.cwd(&spec.cwd);
        for (key, value) in &spec.env {
            command.env(key, value);
        }
        let child = pair.slave.spawn_command(command).map_err(io_error)?;
        drop(pair.slave);
        let reader = pair.master.try_clone_reader().map_err(io_error)?;
        let writer = pair.master.take_writer().map_err(io_error)?;
        let control = NativeControl {
            master: Mutex::new(Some(pair.master)),
            killer: Mutex::new(child.clone_killer()),
            pid: child.process_id(),
        };
        Ok(PtyParts {
            reader,
            writer,
            control: Box::new(control),
            child: Box::new(NativeChild(child)),
        })
    }
}

struct NativeControl {
    master: Mutex<Option<Box<dyn MasterPty + Send>>>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    pid: Option<u32>,
}

impl PtyControl for NativeControl {
    fn resize(&self, cols: u16, rows: u16) -> io::Result<()> {
        let master = self.master.lock().unwrap_or_else(PoisonError::into_inner);
        match master.as_ref() {
            Some(master) => master.resize(size(cols, rows)).map_err(io_error),
            None => Ok(()),
        }
    }

    fn kill(&self) {
        if let Some(pid) = self.pid {
            kill_tree(pid);
        }
        let _ = self
            .killer
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .kill();
    }

    fn release(&self) {
        self.master
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
    }
}

struct NativeChild(Box<dyn portable_pty::Child + Send + Sync>);

impl PtyChild for NativeChild {
    fn wait(&mut self) -> Option<i32> {
        self.0
            .wait()
            .ok()
            .map(|status| i32::from_ne_bytes(status.exit_code().to_ne_bytes()))
    }
}

#[cfg(windows)]
fn kill_tree(pid: u32) {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let _ = Command::new("taskkill")
        .args(["/pid", &pid.to_string(), "/T", "/F"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .status();
}

#[cfg(not(windows))]
const fn kill_tree(_pid: u32) {}
