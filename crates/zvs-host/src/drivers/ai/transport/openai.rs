use std::sync::Arc;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use zvs_contract::AppError;

use super::{
    ApiKey, ApiTransport, ApiTransportOptions, ByteStream, RequestSpec, SecretResolver,
    SharedSecretResolver, Transport, TransportResponse,
};
use crate::drivers::ai::http::HttpClient;
use crate::drivers::ai::openai::{credentials, permission_error};
use crate::services::crypto::Plaintext;

pub struct OpenaiTransport {
    pub secret_id: String,
    pub secrets: SharedSecretResolver,
    pub http: HttpClient,
    pub timeout_seconds: Option<f64>,
}

struct Bearer(String);

impl SecretResolver for Bearer {
    fn resolve(&self, _id: &str) -> Result<Plaintext, AppError> {
        Ok(Plaintext::new(self.0.clone()))
    }
}

impl OpenaiTransport {
    async fn api(&self, cancel: &CancellationToken) -> Result<ApiTransport, AppError> {
        let token = self.oauth_token(cancel).await?;
        let mut options = ApiTransportOptions::new(zvs_openai::API_BASE, self.http.clone());
        options.timeout_seconds = self.timeout_seconds;
        options.api_key = Some(ApiKey::new("openai-oauth", Arc::new(Bearer(token))));
        Ok(ApiTransport::new(options))
    }
}

#[async_trait]
impl Transport for OpenaiTransport {
    fn base_url(&self) -> &str {
        zvs_openai::API_BASE
    }

    async fn oauth_token(&self, cancel: &CancellationToken) -> Result<String, AppError> {
        let saved = credentials(self.secrets.clone(), &self.secret_id, cancel).await?;
        if !saved.allows_inference() {
            return Err(permission_error());
        }
        Ok(saved.access_token)
    }

    async fn request(
        &self,
        spec: &RequestSpec,
        cancel: &CancellationToken,
    ) -> Result<TransportResponse, AppError> {
        self.api(cancel)
            .await?
            .request(spec, cancel)
            .await
            .map_err(oauth_error)
    }

    async fn stream(
        &self,
        spec: &RequestSpec,
        cancel: &CancellationToken,
    ) -> Result<ByteStream, AppError> {
        self.api(cancel)
            .await?
            .stream(spec, cancel)
            .await
            .map_err(oauth_error)
    }
}

fn oauth_error(error: AppError) -> AppError {
    if error.code == zvs_contract::AppErrorCode::ProviderAuthFailed {
        crate::drivers::ai::openai::auth_error()
    } else {
        error
    }
}
