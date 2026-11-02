use std::sync::{Arc, LazyLock};

use lettre::transport::smtp::client::{Certificate, CertificateStore, TlsParameters};
use rustls::pki_types::{CertificateDer, ServerName};
use rustls::{ClientConfig, RootCertStore};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;
use zvs_contract::{AppError, AppErrorCode};

use super::copy;

static SYSTEM_ROOTS: LazyLock<Arc<RootCertStore>> = LazyLock::new(|| {
    let mut store = RootCertStore::empty();
    let loaded = rustls_native_certs::load_native_certs();
    store.add_parsable_certificates(loaded.certs);
    Arc::new(store)
});

#[derive(Clone)]
pub enum MailTrust {
    System,
    Roots(Vec<CertificateDer<'static>>),
}

impl MailTrust {
    fn store(&self) -> Arc<RootCertStore> {
        match self {
            Self::System => Arc::clone(&SYSTEM_ROOTS),
            Self::Roots(certs) => {
                let mut store = RootCertStore::empty();
                store.add_parsable_certificates(certs.iter().cloned());
                Arc::new(store)
            }
        }
    }

    fn client_config(&self) -> Result<Arc<ClientConfig>, AppError> {
        let config =
            ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_safe_default_protocol_versions()
                .map_err(|_| AppError::new(AppErrorCode::NativeError, copy::TLS_FAILED))?
                .with_root_certificates(self.store())
                .with_no_client_auth();
        Ok(Arc::new(config))
    }

    pub async fn wrap(
        &self,
        host: &str,
        stream: TcpStream,
    ) -> Result<TlsStream<TcpStream>, AppError> {
        let name = ServerName::try_from(host.to_owned())
            .map_err(|_| AppError::new(AppErrorCode::ValidationFailed, copy::CONNECT_FAILED))?;
        TlsConnector::from(self.client_config()?)
            .connect(name, stream)
            .await
            .map_err(|_| AppError::new(AppErrorCode::ProviderUnreachable, copy::TLS_FAILED))
    }

    pub fn smtp(&self, host: &str) -> Result<TlsParameters, AppError> {
        let failed = |_| AppError::new(AppErrorCode::NativeError, copy::TLS_FAILED);
        let mut builder = TlsParameters::builder(host.to_owned());
        if let Self::Roots(certs) = self {
            builder = builder.certificate_store(CertificateStore::None);
            for cert in certs {
                builder = builder
                    .add_root_certificate(Certificate::from_der(cert.to_vec()).map_err(failed)?);
            }
        }
        builder.build_rustls().map_err(failed)
    }
}
