use std::sync::Arc;

use zvs_contract::{
    AppErrorCode, AssistantPreferencesDto, ProviderCapability, ProviderKind, SummarizerModelDto,
};

use super::*;
use crate::data::Repositories;
use crate::data::repositories::{ModelDraft, ProviderDraft, ProviderPatch};
use crate::platform::clock::FakeClock;
use crate::testing::{TemporaryDatabase, temporary_database};

struct Setup {
    database: TemporaryDatabase,
    service: AssistantService,
}

impl Setup {
    fn new() -> Self {
        let database = temporary_database();
        let service = AssistantService::new(database.shared(), Arc::new(FakeClock::default()));
        Self { database, service }
    }

    fn provider(&self, port: u16, models: &[ModelDraft]) -> String {
        self.database
            .shared()
            .with_connection(|connection| {
                let repositories = Repositories::new(connection);
                let provider = repositories.providers.create(&ProviderDraft::new(
                    ProviderKind::Ollama,
                    format!("Local {port}"),
                    format!("http://localhost:{port}"),
                    vec![ProviderCapability::Text],
                    1,
                ))?;
                repositories
                    .models
                    .replace_for_provider(&provider.id, models, 1)?;
                Ok(provider.id)
            })
            .expect("seeds a provider")
    }

    fn disable(&self, provider: &str) {
        self.database
            .shared()
            .with_connection(|connection| {
                Repositories::new(connection).providers.update(
                    provider,
                    &ProviderPatch {
                        enabled: Some(false),
                        ..ProviderPatch::default()
                    },
                    2,
                )?;
                Ok(())
            })
            .expect("disables");
    }
}

fn entry(provider: &str, model: &str) -> SummarizerModelDto {
    SummarizerModelDto {
        provider_id: provider.to_owned(),
        model_id: model.to_owned(),
    }
}

fn models(models: Vec<SummarizerModelDto>) -> AssistantPreferencesDto {
    AssistantPreferencesDto {
        summarizer_models: models,
        ..AssistantPreferencesDto::default()
    }
}

#[test]
fn defaults_turn_both_features_on_with_no_models() {
    let setup = Setup::new();
    let preferences = setup.service.get().expect("reads");
    assert_eq!(preferences, AssistantPreferencesDto::default());
    assert!(preferences.auto_compact);
    assert!(preferences.recaps);
    assert!(setup.service.summarizers().is_empty());
}

#[test]
fn set_refuses_a_disabled_provider_and_a_missing_model() {
    let setup = Setup::new();
    let off = setup.provider(11500, &[ModelDraft::new("small", "small", 1)]);
    setup.disable(&off);
    let error = setup
        .service
        .set(&models(vec![entry(&off, "small")]))
        .expect_err("disabled");
    assert_eq!(error.code, AppErrorCode::ValidationFailed);
    assert_eq!(error.message, SUMMARIZER_PROVIDER_UNAVAILABLE);

    let on = setup.provider(11501, &[ModelDraft::new("small", "small", 1)]);
    let error = setup
        .service
        .set(&models(vec![entry(&on, "absent")]))
        .expect_err("missing");
    assert_eq!(error.message, SUMMARIZER_MODEL_UNAVAILABLE);
}

#[test]
fn the_order_is_kept_and_models_that_disappear_are_dropped_on_read() {
    let setup = Setup::new();
    let first = setup.provider(
        11502,
        &[ModelDraft {
            context_window: Some(32_000),
            max_output: Some(2048),
            ..ModelDraft::new("big", "big", 1)
        }],
    );
    let second = setup.provider(11503, &[ModelDraft::new("small", "small", 1)]);
    let stored = setup
        .service
        .set(&AssistantPreferencesDto {
            auto_compact: false,
            recaps: false,
            summarizer_models: vec![entry(&second, "small"), entry(&first, "big")],
        })
        .expect("stores");
    assert_eq!(setup.service.get().expect("reads"), stored);
    let order: Vec<(String, i64, i64)> = setup
        .service
        .summarizers()
        .into_iter()
        .map(|model| {
            (
                model.model_id,
                model.context_window,
                model.max_output_tokens,
            )
        })
        .collect();
    assert_eq!(
        order,
        [
            (
                "small".to_owned(),
                SUMMARIZER_DEFAULT_WINDOW,
                SUMMARIZER_DEFAULT_OUTPUT
            ),
            ("big".to_owned(), 32_000, 2048),
        ]
    );

    setup.disable(&second);
    let read = setup.service.get().expect("reads");
    assert_eq!(read.summarizer_models, [entry(&first, "big")]);
    assert!(!read.auto_compact);
    assert!(!read.recaps);
}
