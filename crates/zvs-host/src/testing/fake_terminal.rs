use std::collections::BTreeSet;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex, PoisonError};

use zvs_contract::TerminalShell;

use crate::terminal::{
    PtyChild, PtyControl, PtyParts, PtySpawner, PtySpec, ShellCommand, ShellLocator,
};

type Exit = Arc<(Mutex<Option<Option<i32>>>, Condvar)>;

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Clone)]
pub struct FakeProcess {
    pub spec: PtySpec,
    output: Arc<Mutex<Option<Sender<Vec<u8>>>>>,
    input: Arc<Mutex<Vec<u8>>>,
    sizes: Arc<Mutex<Vec<(u16, u16)>>>,
    killed: Arc<AtomicBool>,
    exit: Exit,
}

impl FakeProcess {
    pub fn print(&self, bytes: &[u8]) {
        if let Some(sender) = lock(&self.output).as_ref() {
            let _ = sender.send(bytes.to_vec());
        }
    }

    pub fn exit(&self, code: i32) {
        let (slot, ready) = &*self.exit;
        *lock(slot) = Some(Some(code));
        ready.notify_all();
    }

    pub fn input(&self) -> String {
        String::from_utf8_lossy(&lock(&self.input)).into_owned()
    }

    pub fn sizes(&self) -> Vec<(u16, u16)> {
        lock(&self.sizes).clone()
    }

    pub fn killed(&self) -> bool {
        self.killed.load(Ordering::SeqCst)
    }
}

struct ChannelReader {
    receiver: Receiver<Vec<u8>>,
    buffer: Vec<u8>,
}

impl Read for ChannelReader {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.buffer.is_empty() {
            match self.receiver.recv() {
                Ok(bytes) => self.buffer = bytes,
                Err(_) => return Ok(0),
            }
        }
        let count = out.len().min(self.buffer.len());
        out[..count].copy_from_slice(&self.buffer[..count]);
        self.buffer.drain(..count);
        Ok(count)
    }
}

struct SharedWriter(Arc<Mutex<Vec<u8>>>);

impl Write for SharedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        lock(&self.0).extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct FakeControl(FakeProcess);

impl PtyControl for FakeControl {
    fn resize(&self, cols: u16, rows: u16) -> io::Result<()> {
        lock(&self.0.sizes).push((cols, rows));
        Ok(())
    }

    fn kill(&self) {
        self.0.killed.store(true, Ordering::SeqCst);
        self.0.exit(1);
    }

    fn release(&self) {
        lock(&self.0.output).take();
    }
}

struct FakeChild(Exit);

impl PtyChild for FakeChild {
    fn wait(&mut self) -> Option<i32> {
        let (slot, ready) = &*self.0;
        let mut exit = lock(slot);
        while exit.is_none() {
            exit = ready.wait(exit).unwrap_or_else(PoisonError::into_inner);
        }
        exit.unwrap_or(None)
    }
}

#[derive(Default)]
pub struct FakePty {
    processes: Mutex<Vec<FakeProcess>>,
    failing: AtomicBool,
}

impl FakePty {
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn processes(&self) -> Vec<FakeProcess> {
        lock(&self.processes).clone()
    }

    pub fn last(&self) -> FakeProcess {
        self.processes().pop().expect("a spawned process")
    }

    pub fn fail(&self, failing: bool) {
        self.failing.store(failing, Ordering::SeqCst);
    }
}

impl PtySpawner for FakePty {
    fn spawn(&self, spec: &PtySpec) -> io::Result<PtyParts> {
        if self.failing.load(Ordering::SeqCst) {
            return Err(io::Error::other("spawn refused"));
        }
        let (sender, receiver) = channel();
        let process = FakeProcess {
            spec: spec.clone(),
            output: Arc::new(Mutex::new(Some(sender))),
            input: Arc::default(),
            sizes: Arc::default(),
            killed: Arc::default(),
            exit: Arc::default(),
        };
        lock(&self.processes).push(process.clone());
        Ok(PtyParts {
            reader: Box::new(ChannelReader {
                receiver,
                buffer: Vec::new(),
            }),
            writer: Box::new(SharedWriter(Arc::clone(&process.input))),
            child: Box::new(FakeChild(Arc::clone(&process.exit))),
            control: Box::new(FakeControl(process)),
        })
    }
}

pub struct FakeShells(Mutex<BTreeSet<TerminalShell>>);

impl FakeShells {
    pub fn shared(available: &[TerminalShell]) -> Arc<Self> {
        Arc::new(Self(Mutex::new(available.iter().copied().collect())))
    }
}

impl ShellLocator for FakeShells {
    fn locate(&self, shell: TerminalShell) -> Option<ShellCommand> {
        lock(&self.0).contains(&shell).then(|| ShellCommand {
            program: PathBuf::from(format!("{}.exe", shell.as_str())),
            args: vec!["-NoLogo".to_owned()],
        })
    }
}
