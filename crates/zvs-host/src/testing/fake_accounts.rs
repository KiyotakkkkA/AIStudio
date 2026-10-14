use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use async_trait::async_trait;
use tokio::sync::watch;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use zvs_contract::{AccountFamily, AppError, AppErrorCode, Timestamp};

use crate::drivers::ai::identity::{
    AccountIdentity, IdentityProbe, ProbeFactory, ProbeResult, SharedIdentityProbe,
};
use crate::drivers::ai::transport::{AccountToken, SessionGateway};
use crate::platform::clock::Clock;
use crate::platform::navigation::{WorkspaceNavigator, WorkspaceRoute};
use crate::services::accounts::AccountBrowser;

pub const FAKE_IDENTITY_ENDPOINT: &str = "https://example.com/identity";
pub const FAKE_QWEN_LOGIN: &str = "https://chat.qwen.ai/";
pub const FAKE_DEEPSEEK_LOGIN: &str = "https://chat.deepseek.com/";

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

pub fn fake_identity(external_id: &str, token: &str) -> ProbeResult {
    ProbeResult {
        identity: AccountIdentity {
            external_id: external_id.to_owned(),
            email_masked: Some("person@example.com".to_owned()),
            display_name: Some("Person".to_owned()),
            avatar_url: None,
            expires_at: Some(2_000_000_000),
        },
        credential: Some(AccountToken::bearer(token)),
    }
}

pub fn signed_out() -> AppError {
    AppError::new(AppErrorCode::ProviderSessionExpired, "Signed out")
}

#[derive(Debug, Clone)]
pub enum ProbeStep {
    Identity(ProbeResult),
    Fail(AppError),
    Hang,
}

pub struct FakeIdentityProbes {
    queue: Mutex<VecDeque<ProbeStep>>,
    fallback: Mutex<ProbeStep>,
    tokens: Mutex<Vec<CancellationToken>>,
    released: watch::Sender<Option<ProbeResult>>,
}

impl FakeIdentityProbes {
    pub fn shared() -> Arc<Self> {
        Arc::new(Self {
            queue: Mutex::default(),
            fallback: Mutex::new(ProbeStep::Identity(fake_identity(
                "user-1",
                "private-bearer-token",
            ))),
            tokens: Mutex::default(),
            released: watch::channel(None).0,
        })
    }

    pub fn once(&self, step: ProbeStep) -> &Self {
        lock(&self.queue).push_back(step);
        self
    }

    pub fn always(&self, step: ProbeStep) -> &Self {
        *lock(&self.fallback) = step;
        self
    }

    pub fn release(&self, result: ProbeResult) {
        self.released.send_replace(Some(result));
    }

    pub fn calls(&self) -> usize {
        lock(&self.tokens).len()
    }

    pub fn tokens(&self) -> Vec<CancellationToken> {
        lock(&self.tokens).clone()
    }

    pub fn factory(self: &Arc<Self>) -> ProbeFactory {
        let probes = Arc::clone(self);
        Arc::new(move |family| -> SharedIdentityProbe {
            Arc::new(FakeProbe {
                family,
                probes: Arc::clone(&probes),
            })
        })
    }

    async fn next(&self, cancel: &CancellationToken) -> Result<ProbeResult, AppError> {
        lock(&self.tokens).push(cancel.clone());
        let step = lock(&self.queue)
            .pop_front()
            .unwrap_or_else(|| lock(&self.fallback).clone());
        match step {
            ProbeStep::Identity(result) => Ok(result),
            ProbeStep::Fail(error) => Err(error),
            ProbeStep::Hang => {
                let mut released = self.released.subscribe();
                let result = released
                    .wait_for(Option::is_some)
                    .await
                    .ok()
                    .and_then(|value| value.clone());
                result.ok_or_else(signed_out)
            }
        }
    }
}

struct FakeProbe {
    family: AccountFamily,
    probes: Arc<FakeIdentityProbes>,
}

#[async_trait]
impl IdentityProbe for FakeProbe {
    fn family(&self) -> AccountFamily {
        self.family
    }

    fn endpoint(&self) -> &str {
        FAKE_IDENTITY_ENDPOINT
    }

    fn login_url(&self) -> &str {
        match self.family {
            AccountFamily::QwenWeb => FAKE_QWEN_LOGIN,
            AccountFamily::DeepseekWeb => FAKE_DEEPSEEK_LOGIN,
        }
    }

    async fn probe(
        &self,
        _session: &dyn SessionGateway,
        cancel: &CancellationToken,
    ) -> Result<ProbeResult, AppError> {
        self.probes.next(cancel).await
    }
}

#[derive(Default)]
pub struct RecordingBrowser {
    opened: Mutex<Vec<String>>,
    failure: Mutex<Option<AppError>>,
}

impl RecordingBrowser {
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn opened(&self) -> Vec<String> {
        lock(&self.opened).clone()
    }

    pub fn fail_with(&self, error: AppError) {
        *lock(&self.failure) = Some(error);
    }
}

#[async_trait]
impl AccountBrowser for RecordingBrowser {
    async fn open_tab(&self, url: &str) -> Result<(), AppError> {
        if let Some(error) = lock(&self.failure).clone() {
            return Err(error);
        }
        lock(&self.opened).push(url.to_owned());
        Ok(())
    }
}

#[derive(Default)]
pub struct RecordingNavigator(Mutex<Vec<WorkspaceRoute>>);

impl RecordingNavigator {
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn routes(&self) -> Vec<WorkspaceRoute> {
        lock(&self.0).clone()
    }
}

impl WorkspaceNavigator for RecordingNavigator {
    fn navigate(&self, route: WorkspaceRoute) {
        lock(&self.0).push(route);
    }
}

#[derive(Debug)]
pub struct TokioClock {
    origin: Timestamp,
    start: Instant,
}

impl TokioClock {
    pub fn new(origin: Timestamp) -> Self {
        Self {
            origin,
            start: Instant::now(),
        }
    }
}

impl Clock for TokioClock {
    fn now(&self) -> Timestamp {
        let elapsed = Instant::now().duration_since(self.start).as_millis();
        self.origin + Timestamp::try_from(elapsed).unwrap_or(Timestamp::MAX)
    }
}
