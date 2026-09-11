#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("IO failure: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("Backend failure: {0}")]
    Backend(String),
    #[error("Operation cancelled")]
    Cancelled,
}

impl Error {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Io(error) => match error.kind() {
                std::io::ErrorKind::NotFound => "NOT_FOUND",
                std::io::ErrorKind::PermissionDenied => "PERMISSION_DENIED",
                _ => "NATIVE_ERROR",
            },
            Self::InvalidInput(_) => "VALIDATION_FAILED",
            Self::Backend(_) => "NATIVE_ERROR",
            Self::Cancelled => "RUN_CANCELLED",
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

pub(crate) fn check_cancelled(token: &crate::CancellationToken) -> Result<()> {
    if token.is_cancelled() {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}
