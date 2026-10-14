use async_trait::async_trait;
use futures::StreamExt;
use tokio_util::sync::CancellationToken;
use zvs_contract::{AdapterFamily, AppError, AuthMode, ModelCapability, ToolMode};

use super::account_family::{
    AccountAdapterCore, AccountAdapterOptions, VendorObservation, generation_not_implemented,
};
use crate::drivers::ai::capabilities::{AdapterCapabilities, Honours};
use crate::drivers::ai::ports::{
    DeltaStream, DiscoveredModel, GenerateRequest, GenerateResult, TextGenerationDriver,
};

pub const DEEPSEEK_MODELS_OBSERVATION: VendorObservation = VendorObservation {
    source: "https://chat.deepseek.com/ — model picker; no model-list endpoint is exposed",
    observed_at: "2026-09-10",
};

pub const DEEPSEEK_GENERATION_OBSERVATION: VendorObservation = VendorObservation {
    source: "https://chat.deepseek.com/ — completion wire format not yet recorded",
    observed_at: "2026-09-10",
};

pub const DEEPSEEK_WEB_CAPABILITIES: AdapterCapabilities = AdapterCapabilities {
    family: AdapterFamily::DeepseekWeb,
    auth_modes: &[AuthMode::Account],
    streaming: false,
    live_model_list: false,
    embedding: false,
    image: false,
    honours: Honours::NONE,
    tools: ToolMode::Injected,
};

pub fn deepseek_curated_models() -> Vec<DiscoveredModel> {
    vec![
        DiscoveredModel {
            display_name: "DeepSeek".to_owned(),
            family: Some("deepseek".to_owned()),
            capabilities: vec![ModelCapability::Streaming],
            ..DiscoveredModel::named("deepseek-chat")
        },
        DiscoveredModel {
            display_name: "DeepSeek (DeepThink)".to_owned(),
            family: Some("deepseek".to_owned()),
            capabilities: vec![ModelCapability::Streaming, ModelCapability::Reasoning],
            ..DiscoveredModel::named("deepseek-reasoner")
        },
    ]
}

pub struct DeepSeekWebAdapter {
    core: AccountAdapterCore,
}

impl DeepSeekWebAdapter {
    pub fn new(options: AccountAdapterOptions) -> Self {
        Self {
            core: AccountAdapterCore::new(options, DEEPSEEK_WEB_CAPABILITIES),
        }
    }

    fn pending(&self, request: &GenerateRequest) -> AppError {
        generation_not_implemented(self.core.capabilities.family, &request.model)
    }
}

#[async_trait]
impl TextGenerationDriver for DeepSeekWebAdapter {
    fn capabilities(&self) -> AdapterCapabilities {
        DEEPSEEK_WEB_CAPABILITIES
    }

    async fn generate(
        &self,
        request: &GenerateRequest,
        _cancel: &CancellationToken,
    ) -> Result<GenerateResult, AppError> {
        Err(self.pending(request))
    }

    fn stream(&self, request: &GenerateRequest, _cancel: &CancellationToken) -> DeltaStream {
        futures::stream::iter([Err(self.pending(request))]).boxed()
    }

    async fn list_models(
        &self,
        _cancel: &CancellationToken,
    ) -> Result<Vec<DiscoveredModel>, AppError> {
        Ok(deepseek_curated_models())
    }
}
