use std::fmt;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use zvs_contract::{AppError, AppErrorCode};

#[derive(Clone, PartialEq, Eq)]
pub struct UploadFile {
    pub name: String,
    pub mime_type: String,
    pub bytes: Arc<[u8]>,
}

impl fmt::Debug for UploadFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UploadFile")
            .field("bytes", &self.bytes.len())
            .finish_non_exhaustive()
    }
}

pub fn upload_failed() -> AppError {
    AppError::new(
        AppErrorCode::ProviderUnreachable,
        crate::images::UPLOAD_FAILED,
    )
}

fn object_client() -> Result<&'static reqwest::Client, AppError> {
    static CLIENT: OnceLock<Result<reqwest::Client, AppError>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .use_rustls_tls()
                .tls_built_in_native_certs(true)
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(120))
                .build()
                .map_err(|_| upload_failed())
        })
        .as_ref()
        .map_err(Clone::clone)
}

/// Object storage receives only the temporary grant's headers, never an account bearer or cookies.
pub async fn put_object(
    url: &str,
    headers: &[(String, String)],
    bytes: Arc<[u8]>,
    cancel: &CancellationToken,
) -> Result<(), AppError> {
    let client = object_client()?;
    let mut request = client.put(url).body(bytes.to_vec());
    for (name, value) in headers {
        request = request.header(name, value);
    }
    tokio::select! {
        biased;
        () = cancel.cancelled() => Err(super::super::errors::cancelled()),
        response = request.send() => match response {
            Ok(response) if response.status().is_success() => Ok(()),
            _ => Err(upload_failed()),
        }
    }
}
