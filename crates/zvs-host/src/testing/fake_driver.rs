use std::future::{Future, pending};
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use futures::stream;
use tokio::sync::{Notify, oneshot};
use tokio_util::sync::CancellationToken;
use zvs_contract::{AdapterFamily, AppError, AuthMode, ToolMode};

use crate::data::schema::ProviderRow;
use crate::drivers::ai::capabilities::{AdapterCapabilities, Honours};
use crate::drivers::ai::errors::cancelled;
use crate::drivers::ai::ports::{
    AiDriver, DeltaStream, DiscoveredModel, EmbeddingDriver, FinishReason, GenerateRequest,
    GenerateResult, GeneratedImage, ImageDriver, ImageRequest, TextDelta, TextGenerationDriver,
};
use crate::services::providers::DriverSource;

pub const FAKE_CAPABILITIES: AdapterCapabilities = AdapterCapabilities {
    family: AdapterFamily::OpenaiCompatible,
    auth_modes: &[AuthMode::Api, AuthMode::Account],
    streaming: true,
    live_model_list: true,
    embedding: true,
    image: true,
    honours: Honours {
        temperature: true,
        top_k: false,
        top_p: true,
        max_output_tokens: true,
    },
    tools: ToolMode::Native,
};

#[derive(Debug, Clone, Default)]
pub struct FakeScript {
    pub models: Option<Vec<DiscoveredModel>>,
    pub deltas: Option<Vec<String>>,
    pub fail_with: Option<AppError>,
    pub delay: Option<Duration>,
    pub manual: Option<bool>,
    pub vectors: Option<Vec<Vec<f32>>>,
}

#[derive(Default)]
struct FakeState {
    script: FakeScript,
    gate: Option<oneshot::Sender<()>>,
    requests: Vec<GenerateRequest>,
    embeddings: Vec<(Vec<String>, String)>,
    list_calls: usize,
}

#[derive(Default)]
pub struct FakeDriver {
    state: Mutex<FakeState>,
    holding: Notify,
}

impl FakeDriver {
    pub fn shared(script: FakeScript) -> Arc<Self> {
        let driver = Self::default();
        driver.script(script);
        Arc::new(driver)
    }

    pub fn with_models(models: Vec<DiscoveredModel>) -> Arc<Self> {
        Self::shared(FakeScript {
            models: Some(models),
            ..FakeScript::default()
        })
    }

    fn state(&self) -> MutexGuard<'_, FakeState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn script(&self, next: FakeScript) {
        let mut state = self.state();
        let script = &mut state.script;
        if next.models.is_some() {
            script.models = next.models;
        }
        if next.deltas.is_some() {
            script.deltas = next.deltas;
        }
        if next.fail_with.is_some() {
            script.fail_with = next.fail_with;
        }
        if next.delay.is_some() {
            script.delay = next.delay;
        }
        if next.manual.is_some() {
            script.manual = next.manual;
        }
        if next.vectors.is_some() {
            script.vectors = next.vectors;
        }
    }

    pub fn clear_failure(&self) {
        self.state().script.fail_with = None;
    }

    pub fn requests(&self) -> Vec<GenerateRequest> {
        self.state().requests.clone()
    }

    pub fn with_vectors(vectors: Vec<Vec<f32>>) -> Arc<Self> {
        Self::shared(FakeScript {
            vectors: Some(vectors),
            ..FakeScript::default()
        })
    }

    pub fn embeddings(&self) -> Vec<(Vec<String>, String)> {
        self.state().embeddings.clone()
    }

    pub fn list_calls(&self) -> usize {
        self.state().list_calls
    }

    pub fn is_holding(&self) -> bool {
        self.state().gate.is_some()
    }

    pub async fn when_holding(&self) {
        loop {
            let notified = self.holding.notified();
            if self.is_holding() {
                return;
            }
            notified.await;
        }
    }

    pub fn release(&self) {
        if let Some(gate) = self.state().gate.take() {
            let _ = gate.send(());
        }
    }

    pub fn as_driver(self: &Arc<Self>) -> AiDriver {
        AiDriver {
            text: Some(self.clone()),
            embedding: Some(self.clone()),
            image: Some(self.clone()),
        }
    }

    async fn hold(&self, cancel: &CancellationToken) -> Result<(), AppError> {
        let (delay, manual) = {
            let state = self.state();
            (state.script.delay, state.script.manual.unwrap_or(false))
        };
        if delay.is_none() && !manual {
            return if cancel.is_cancelled() {
                Err(cancelled())
            } else {
                Ok(())
            };
        }
        let (open, released) = oneshot::channel();
        self.state().gate = Some(open);
        self.holding.notify_waiters();
        let timer: Pin<Box<dyn Future<Output = ()> + Send>> = match delay {
            Some(delay) => Box::pin(tokio::time::sleep(delay)),
            None => Box::pin(pending()),
        };
        let outcome = tokio::select! {
            () = cancel.cancelled() => Err(cancelled()),
            _ = released => Ok(()),
            () = timer => Ok(()),
        };
        self.state().gate = None;
        outcome
    }

    fn failure(&self) -> Result<(), AppError> {
        self.state().script.fail_with.clone().map_or(Ok(()), Err)
    }

    async fn list(&self, cancel: &CancellationToken) -> Result<Vec<DiscoveredModel>, AppError> {
        self.state().list_calls += 1;
        self.hold(cancel).await?;
        self.failure()?;
        Ok(self.state().script.models.clone().unwrap_or_default())
    }
}

#[async_trait]
impl TextGenerationDriver for FakeDriver {
    fn capabilities(&self) -> AdapterCapabilities {
        FAKE_CAPABILITIES
    }

    async fn generate(
        &self,
        request: &GenerateRequest,
        cancel: &CancellationToken,
    ) -> Result<GenerateResult, AppError> {
        self.state().requests.push(request.clone());
        self.hold(cancel).await?;
        self.failure()?;
        let text = self
            .state()
            .script
            .deltas
            .clone()
            .unwrap_or_default()
            .concat();
        Ok(GenerateResult {
            model: request.model.clone(),
            text,
            reasoning: None,
            finish_reason: FinishReason::Stop,
            usage: None,
            tool_calls: Vec::new(),
        })
    }

    fn stream(&self, request: &GenerateRequest, _cancel: &CancellationToken) -> DeltaStream {
        let deltas = {
            let mut state = self.state();
            state.requests.push(request.clone());
            state.script.deltas.clone().unwrap_or_default()
        };
        Box::pin(stream::iter(
            deltas.into_iter().map(|delta| Ok(TextDelta::text(delta))),
        ))
    }

    async fn list_models(
        &self,
        cancel: &CancellationToken,
    ) -> Result<Vec<DiscoveredModel>, AppError> {
        self.list(cancel).await
    }
}

#[async_trait]
impl EmbeddingDriver for FakeDriver {
    fn capabilities(&self) -> AdapterCapabilities {
        FAKE_CAPABILITIES
    }

    async fn embed(
        &self,
        texts: &[String],
        model: &str,
        cancel: &CancellationToken,
    ) -> Result<Vec<Vec<f32>>, AppError> {
        self.state()
            .embeddings
            .push((texts.to_vec(), model.to_owned()));
        self.hold(cancel).await?;
        self.failure()?;
        Ok(self
            .state()
            .script
            .vectors
            .clone()
            .unwrap_or_else(|| texts.iter().map(|_| vec![0.0]).collect()))
    }

    fn dimensions(&self) -> Option<usize> {
        None
    }

    async fn list_models(
        &self,
        cancel: &CancellationToken,
    ) -> Result<Vec<DiscoveredModel>, AppError> {
        self.list(cancel).await
    }
}

#[async_trait]
impl ImageDriver for FakeDriver {
    fn capabilities(&self) -> AdapterCapabilities {
        FAKE_CAPABILITIES
    }

    async fn generate_images(
        &self,
        _request: &ImageRequest,
        cancel: &CancellationToken,
    ) -> Result<Vec<GeneratedImage>, AppError> {
        self.hold(cancel).await?;
        self.failure()?;
        Ok(Vec::new())
    }

    async fn list_models(
        &self,
        cancel: &CancellationToken,
    ) -> Result<Vec<DiscoveredModel>, AppError> {
        self.list(cancel).await
    }
}

#[derive(Default)]
struct SourceState {
    driver: Option<Arc<FakeDriver>>,
    stalled: bool,
    invalidated: Vec<String>,
    built: usize,
}

#[derive(Default)]
pub struct FakeDriverSource(Mutex<SourceState>);

impl FakeDriverSource {
    pub fn shared(driver: Arc<FakeDriver>) -> Arc<Self> {
        let source = Self::default();
        source.set(driver);
        Arc::new(source)
    }

    pub fn stalled() -> Arc<Self> {
        let source = Self::default();
        source.state().stalled = true;
        Arc::new(source)
    }

    fn state(&self) -> MutexGuard<'_, SourceState> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn set(&self, driver: Arc<FakeDriver>) {
        self.state().driver = Some(driver);
    }

    pub fn driver(&self) -> Option<Arc<FakeDriver>> {
        self.state().driver.clone()
    }

    pub fn invalidated(&self) -> Vec<String> {
        self.state().invalidated.clone()
    }

    pub fn built(&self) -> usize {
        self.state().built
    }
}

#[async_trait]
impl DriverSource for FakeDriverSource {
    async fn ephemeral_driver(&self, _row: &ProviderRow) -> Result<AiDriver, AppError> {
        let (driver, stalled) = {
            let mut state = self.state();
            state.built += 1;
            (state.driver.clone(), state.stalled)
        };
        if stalled {
            pending::<()>().await;
        }
        Ok(driver.map(|driver| driver.as_driver()).unwrap_or_default())
    }

    fn invalidate(&self, provider_id: &str) {
        self.state().invalidated.push(provider_id.to_owned());
    }
}
