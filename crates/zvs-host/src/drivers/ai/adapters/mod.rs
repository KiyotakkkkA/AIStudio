pub mod local;
pub mod ollama;
pub mod openai_compatible;
pub mod wire;

use std::sync::Arc;

use zvs_contract::{AdapterFamily, AppError, AppErrorCode, AuthMode, ToolMode};

pub use local::{
    FileSystemModelStore, LOCAL_CAPABILITIES, LocalEmbeddingAdapter, LocalEmbeddingEngine,
    LocalModelStore, SharedEmbeddingEngine, SharedModelStore,
};
pub use ollama::{OLLAMA_CAPABILITIES, OllamaAdapter};
pub use openai_compatible::{
    OPENAI_COMPATIBLE_CAPABILITIES, OpenAiCompatibleAdapter, ToolCallAssembler,
};

use super::capabilities::{AdapterCapabilities, Honours};
use super::ports::AiDriver;
use super::transport::SharedTransport;
use crate::platform::logger::{SharedLogger, silent_logger};

pub const QWEN_WEB_CAPABILITIES: AdapterCapabilities = AdapterCapabilities {
    family: AdapterFamily::QwenWeb,
    auth_modes: &[AuthMode::Account],
    streaming: true,
    live_model_list: true,
    embedding: false,
    image: false,
    honours: Honours::NONE,
    tools: ToolMode::Injected,
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

#[derive(Clone)]
pub struct AdapterContext {
    pub transport: Option<SharedTransport>,
    pub logger: SharedLogger,
    pub local_models: Option<SharedModelStore>,
    pub local_engine: Option<SharedEmbeddingEngine>,
}

impl Default for AdapterContext {
    fn default() -> Self {
        Self {
            transport: None,
            logger: silent_logger(),
            local_models: None,
            local_engine: None,
        }
    }
}

impl AdapterContext {
    pub fn with_transport(transport: SharedTransport) -> Self {
        Self {
            transport: Some(transport),
            ..Self::default()
        }
    }

    fn require_transport(&self) -> Result<SharedTransport, AppError> {
        self.transport
            .clone()
            .ok_or_else(|| AppError::new(AppErrorCode::Conflict, "Адаптеру нужен транспорт"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdapterFamilyEntry {
    pub capabilities: AdapterCapabilities,
    pub implemented: bool,
}

pub fn not_implemented(family: &str) -> AppError {
    AppError::new(
        AppErrorCode::Unknown,
        format!("adapter {family} is not implemented yet"),
    )
    .with_detail("family", family)
}

impl AdapterFamilyEntry {
    pub fn build(&self, context: &AdapterContext) -> Result<AiDriver, AppError> {
        let family = self.capabilities.family;
        match family {
            AdapterFamily::OpenaiCompatible => {
                let adapter = Arc::new(OpenAiCompatibleAdapter::new(
                    context.require_transport()?,
                    context.logger.clone(),
                ));
                Ok(AiDriver {
                    text: Some(adapter.clone()),
                    embedding: Some(adapter),
                    image: None,
                })
            }
            AdapterFamily::Local => {
                let models = context.local_models.clone().ok_or_else(|| {
                    AppError::new(
                        AppErrorCode::Conflict,
                        "Каталог локальных моделей недоступен",
                    )
                })?;
                Ok(AiDriver {
                    text: None,
                    embedding: Some(Arc::new(LocalEmbeddingAdapter::new(
                        models,
                        context.local_engine.clone(),
                        context.logger.clone(),
                    ))),
                    image: None,
                })
            }
            AdapterFamily::QwenWeb | AdapterFamily::DeepseekWeb => {
                context.require_transport()?;
                Err(not_implemented(family.as_str()))
            }
        }
    }
}

pub const fn adapter_entry(family: AdapterFamily) -> AdapterFamilyEntry {
    match family {
        AdapterFamily::OpenaiCompatible => AdapterFamilyEntry {
            capabilities: OPENAI_COMPATIBLE_CAPABILITIES,
            implemented: true,
        },
        AdapterFamily::Local => AdapterFamilyEntry {
            capabilities: LOCAL_CAPABILITIES,
            implemented: true,
        },
        AdapterFamily::QwenWeb => AdapterFamilyEntry {
            capabilities: QWEN_WEB_CAPABILITIES,
            implemented: false,
        },
        AdapterFamily::DeepseekWeb => AdapterFamilyEntry {
            capabilities: DEEPSEEK_WEB_CAPABILITIES,
            implemented: false,
        },
    }
}

pub fn adapter_entry_named(family: &str) -> Result<AdapterFamilyEntry, AppError> {
    AdapterFamily::parse(family)
        .map(adapter_entry)
        .ok_or_else(|| not_implemented(family))
}

pub const fn adapter_capabilities(family: AdapterFamily) -> AdapterCapabilities {
    adapter_entry(family).capabilities
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drivers::ai::mock::transport_for;
    use crate::testing::temporary_directory;

    #[test]
    fn every_family_has_an_entry_and_an_unknown_one_is_refused() {
        for family in AdapterFamily::ALL {
            assert_eq!(adapter_entry(*family).capabilities.family, *family);
        }
        assert!(adapter_entry(AdapterFamily::OpenaiCompatible).implemented);
        assert!(adapter_entry(AdapterFamily::Local).implemented);
        assert!(!adapter_entry(AdapterFamily::QwenWeb).implemented);
        assert!(!adapter_entry(AdapterFamily::DeepseekWeb).implemented);
        assert!(adapter_capabilities(AdapterFamily::Local).embedding);
        assert!(!adapter_capabilities(AdapterFamily::Local).streaming);
        let error = adapter_entry_named("made-up").expect_err("refuses");
        assert_eq!(error.code, AppErrorCode::Unknown);
        assert_eq!(error.message, "adapter made-up is not implemented yet");
        assert!(
            adapter_capabilities(AdapterFamily::QwenWeb)
                .auth_modes
                .contains(&AuthMode::Account)
        );
        assert!(
            !adapter_capabilities(AdapterFamily::OpenaiCompatible)
                .honours
                .top_k
        );
        assert_eq!(
            adapter_entry_named("openai-compatible").expect("known"),
            adapter_entry(AdapterFamily::OpenaiCompatible)
        );
    }

    #[test]
    fn the_openai_family_builds_text_and_embedding_drivers_over_its_transport() {
        let entry = adapter_entry(AdapterFamily::OpenaiCompatible);
        let driver = entry
            .build(&AdapterContext::with_transport(transport_for(
                "http://127.0.0.1:9/v1",
            )))
            .expect("builds");
        assert!(driver.text.is_some());
        assert!(driver.embedding.is_some());
        assert!(driver.image.is_none());
        let missing = entry
            .build(&AdapterContext::default())
            .expect_err("refuses");
        assert_eq!(missing.code, AppErrorCode::Conflict);
        assert_eq!(missing.message, "Адаптеру нужен транспорт");
    }

    #[test]
    fn the_local_entry_builds_an_embedding_only_driver_and_needs_no_transport() {
        let workspace = temporary_directory("local-models-");
        let entry = adapter_entry(AdapterFamily::Local);
        let driver = entry
            .build(&AdapterContext {
                local_models: Some(Arc::new(FileSystemModelStore::new(workspace.path()))),
                ..AdapterContext::default()
            })
            .expect("builds");
        assert!(driver.text.is_none());
        assert!(driver.image.is_none());
        assert!(driver.embedding.is_some());
        let error = entry
            .build(&AdapterContext::default())
            .expect_err("refuses");
        assert_eq!(error.message, "Каталог локальных моделей недоступен");
    }

    #[test]
    fn account_families_wait_for_their_port() {
        let error = adapter_entry(AdapterFamily::QwenWeb)
            .build(&AdapterContext::with_transport(transport_for(
                "http://127.0.0.1:9",
            )))
            .expect_err("not yet");
        assert_eq!(error.message, "adapter qwen-web is not implemented yet");
    }
}
