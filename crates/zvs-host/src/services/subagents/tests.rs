use std::sync::Arc;

use serde_json::json;
use zvs_contract::{
    AppErrorCode, AuthMode, ModelCapability, ProviderCapability, ProviderKind, ProviderStatus,
    SubagentModelDto, SubagentPreferencesDto,
};

use super::*;
use crate::data::Repositories;
use crate::data::repositories::{ModelDraft, ProviderDraft, ProviderFilter, ProviderPatch};
use crate::platform::clock::FakeClock;
use crate::testing::{TemporaryDatabase, temporary_database};

struct Setup {
    database: TemporaryDatabase,
    service: SubagentService,
}

fn tooled(external: &str) -> ModelDraft {
    ModelDraft {
        capabilities: vec![ModelCapability::Tools],
        ..ModelDraft::new(external, external, 1)
    }
}

impl Setup {
    fn new() -> Self {
        let database = temporary_database();
        let service = SubagentService::new(database.shared(), Arc::new(FakeClock::default()));
        Self { database, service }
    }

    fn provider(&self, draft: ProviderDraft, models: &[ModelDraft]) -> String {
        self.database
            .shared()
            .with_connection(|connection| {
                let repositories = Repositories::new(connection);
                let provider = repositories.providers.create(&draft)?;
                repositories
                    .models
                    .replace_for_provider(&provider.id, models, 1)?;
                Ok(provider.id)
            })
            .expect("seeds a provider")
    }

    fn api(&self, models: &[ModelDraft]) -> String {
        let index = self.count();
        self.provider(
            ProviderDraft::new(
                ProviderKind::Ollama,
                format!("Local {index}"),
                format!("http://localhost:{}", 11434 + index),
                vec![ProviderCapability::Text],
                1,
            ),
            models,
        )
    }

    fn account(&self, models: &[ModelDraft]) -> String {
        self.provider(
            ProviderDraft {
                auth_mode: AuthMode::Account,
                status: ProviderStatus::NeedsRelink,
                ..ProviderDraft::new(
                    ProviderKind::OpenaiCompatible,
                    "Account",
                    "https://example.test",
                    vec![ProviderCapability::Text],
                    1,
                )
            },
            models,
        )
    }

    fn count(&self) -> usize {
        self.database
            .shared()
            .with_connection(|connection| {
                Ok(Repositories::new(connection)
                    .providers
                    .list(&ProviderFilter::default())?
                    .len())
            })
            .expect("counts")
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

fn entry(provider: &str, model: &str) -> SubagentModelDto {
    SubagentModelDto {
        provider_id: provider.to_owned(),
        model_id: model.to_owned(),
    }
}

fn preferences(models: Vec<SubagentModelDto>) -> SubagentPreferencesDto {
    SubagentPreferencesDto {
        models,
        ..SubagentPreferencesDto::default()
    }
}

#[test]
fn defaults_apply_when_nothing_is_stored() {
    let setup = Setup::new();
    assert_eq!(
        setup.service.get().expect("reads"),
        SubagentPreferencesDto::default()
    );
    assert_eq!(setup.service.budget(), SubagentBudget::default());
}

#[test]
fn set_refuses_a_disabled_provider_a_missing_model_and_a_model_without_tools() {
    let setup = Setup::new();
    let off = setup.api(&[tooled("cheap")]);
    setup.disable(&off);
    let error = setup
        .service
        .set(&preferences(vec![entry(&off, "cheap")]))
        .expect_err("refuses");
    assert_eq!(error.code, AppErrorCode::ValidationFailed);
    assert_eq!(error.message, SUBAGENT_PROVIDER_UNAVAILABLE);

    let live = setup.api(&[tooled("cheap"), ModelDraft::new("plain", "plain", 1)]);
    let error = setup
        .service
        .set(&preferences(vec![entry(&live, "absent")]))
        .expect_err("refuses");
    assert_eq!(error.message, SUBAGENT_MODEL_UNAVAILABLE);

    let error = setup
        .service
        .set(&preferences(vec![entry(&live, "plain")]))
        .expect_err("refuses");
    assert_eq!(error.message, SUBAGENT_MODEL_NO_TOOLS);

    let saved = setup
        .service
        .set(&preferences(vec![entry(&live, "cheap")]))
        .expect("stores");
    assert_eq!(setup.service.get().expect("reads"), saved);
}

#[test]
fn set_refuses_duplicates_and_out_of_range_concurrency() {
    let setup = Setup::new();
    let live = setup.api(&[tooled("cheap")]);
    let duplicated = preferences(vec![entry(&live, "cheap"), entry(&live, "cheap")]);
    assert_eq!(
        setup.service.set(&duplicated).expect_err("refuses").code,
        AppErrorCode::ValidationFailed
    );
    let crowded = SubagentPreferencesDto {
        concurrency: 5,
        ..SubagentPreferencesDto::default()
    };
    assert!(setup.service.set(&crowded).is_err());
}

#[test]
fn get_drops_a_model_that_has_since_disappeared_and_stores_the_result() {
    let setup = Setup::new();
    let first = setup.api(&[tooled("cheap")]);
    let second = setup.api(&[tooled("strong")]);
    setup
        .service
        .set(&preferences(vec![
            entry(&first, "cheap"),
            entry(&second, "strong"),
        ]))
        .expect("stores");
    setup.disable(&first);

    let sanitized = setup.service.get().expect("reads");

    assert_eq!(sanitized.models, [entry(&second, "strong")]);
    let stored = setup
        .database
        .shared()
        .with_connection(|connection| {
            Ok(Repositories::new(connection).settings.get(SUBAGENTS_KEY)?)
        })
        .expect("reads")
        .expect("a stored value");
    let stored: SubagentPreferencesDto = serde_json::from_str(&stored.value).expect("parses");
    assert_eq!(stored, sanitized);
}

#[test]
fn selection_takes_the_first_usable_model_in_order_and_reports_account_mode() {
    let setup = Setup::new();
    let first = setup.api(&[tooled("cheap")]);
    let account = setup.account(&[tooled("web")]);
    setup
        .service
        .set(&preferences(vec![
            entry(&first, "cheap"),
            entry(&account, "web"),
        ]))
        .expect("stores");

    let chosen = setup
        .service
        .selected(None)
        .expect("selects")
        .expect("a model");
    assert_eq!(chosen.external_id, "cheap");
    assert!(!chosen.account);

    setup.disable(&first);
    let chosen = setup
        .service
        .selected(None)
        .expect("selects")
        .expect("a model");
    assert_eq!(chosen.external_id, "web");
    assert!(chosen.account);
    assert!(account_provider(setup.database.shared().as_ref(), &account).expect("reads"));

    setup.disable(&account);
    assert_eq!(setup.service.selected(None).expect("selects"), None);
}

#[test]
fn budget_reads_the_settings_and_ignores_zero() {
    let setup = Setup::new();
    let settings = SettingService::new(setup.database.shared(), Arc::new(FakeClock::default()));
    settings.set(BUDGET_CALLS_KEY, &json!(3)).expect("stores");
    settings.set(BUDGET_TOKENS_KEY, &json!(0)).expect("stores");
    settings.set(BUDGET_MINUTES_KEY, &json!(2)).expect("stores");
    assert_eq!(
        setup.service.budget(),
        SubagentBudget {
            tool_calls: 3,
            tokens: DEFAULT_BUDGET_TOKENS,
            minutes: 2,
        }
    );
}
