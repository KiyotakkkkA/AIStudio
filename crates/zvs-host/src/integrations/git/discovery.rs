use zvs_contract::integrations::GitRepositoryDto;
use zvs_contract::{AppError, AppErrorCode};

use super::forge::Forge;
use super::{GitHostingProvider, IntegrationConfigView, settings};

impl GitHostingProvider {
    pub(super) async fn load_repositories(
        &self,
        account: &str,
        config: &IntegrationConfigView,
    ) -> Result<Vec<GitRepositoryDto>, AppError> {
        let settings = settings::load(self.forge, config)?;
        let bearer = self.bearer(config, &settings).await?;
        let client = self.client(&settings, &bearer, account);
        let path = match self.forge {
            Forge::GitHub => "/user/repos?affiliation=owner,collaborator,organization_member",
            Forge::GitLab => "/projects?membership=true",
        };
        let mut repositories = Vec::new();
        for page in 1..=100 {
            let body = client
                .retrying_get(&format!("{path}&per_page=100&page={page}"))
                .await?;
            let entries = body.as_array().ok_or_else(|| {
                AppError::new(
                    AppErrorCode::ProviderUnreachable,
                    super::copy::UNEXPECTED_REPLY,
                )
            })?;
            for entry in entries {
                let name = match self.forge {
                    Forge::GitHub => entry["full_name"].as_str(),
                    Forge::GitLab => entry["path_with_namespace"].as_str(),
                }
                .ok_or_else(|| {
                    AppError::new(
                        AppErrorCode::ProviderUnreachable,
                        super::copy::UNEXPECTED_REPLY,
                    )
                })?;
                repositories.push(GitRepositoryDto {
                    id: name.to_owned(),
                    name: name.to_owned(),
                });
            }
            if entries.len() < 100 {
                repositories.sort_by(|left, right| left.name.cmp(&right.name));
                repositories.dedup_by(|left, right| left.id == right.id);
                return Ok(repositories);
            }
        }
        Err(AppError::new(
            AppErrorCode::LimitExceeded,
            "Слишком много репозиториев для загрузки",
        ))
    }
}
