use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

pub const LEDGER_RUNS: usize = 64;

#[derive(Default)]
struct State {
    runs: VecDeque<(String, HashMap<String, String>)>,
}

#[derive(Clone, Default)]
pub struct ReadLedger {
    state: Arc<Mutex<State>>,
}

fn key(path: &Path) -> String {
    path.to_string_lossy().to_lowercase()
}

impl ReadLedger {
    pub fn new() -> Self {
        Self::default()
    }

    fn with<T>(&self, work: impl FnOnce(&mut State) -> T) -> T {
        work(&mut self.state.lock().unwrap_or_else(PoisonError::into_inner))
    }

    pub fn record(&self, run: &str, path: &Path, hash: &str) {
        self.with(|state| {
            if let Some(index) = state.runs.iter().position(|(id, _)| id == run) {
                if let Some(entry) = state.runs.remove(index) {
                    state.runs.push_back(entry);
                }
            } else {
                if state.runs.len() >= LEDGER_RUNS {
                    state.runs.pop_front();
                }
                state.runs.push_back((run.to_owned(), HashMap::new()));
            }
            if let Some((_, hashes)) = state.runs.back_mut() {
                hashes.insert(key(path), hash.to_owned());
            }
        });
    }

    pub fn hash(&self, run: &str, path: &Path) -> Option<String> {
        self.with(|state| {
            state
                .runs
                .iter()
                .find(|(id, _)| id == run)
                .and_then(|(_, hashes)| hashes.get(&key(path)).cloned())
        })
    }

    pub fn forget(&self, run: &str, path: &Path) {
        self.with(|state| {
            if let Some((_, hashes)) = state.runs.iter_mut().find(|(id, _)| id == run) {
                hashes.remove(&key(path));
            }
        });
    }
}
