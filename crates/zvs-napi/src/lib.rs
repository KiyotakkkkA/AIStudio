use std::panic::{UnwindSafe, catch_unwind};

use napi::bindgen_prelude::Buffer;
use napi_derive::napi;
use zvs_core::{CancellationToken, Error as CoreError, chunk, hash};

pub mod vector;

struct CoreFailure(CoreError);

impl From<CoreFailure> for napi::Error {
    fn from(error: CoreFailure) -> Self {
        native_error(error.0.code(), &error.0.to_string())
    }
}

fn native_error(code: &str, message: &str) -> napi::Error {
    napi::Error::from_reason(serde_json::json!({ "code": code, "message": message }).to_string())
}

fn guarded<T>(work: impl FnOnce() -> zvs_core::Result<T> + UnwindSafe) -> napi::Result<T> {
    match catch_unwind(work) {
        Ok(result) => result.map_err(|error| CoreFailure(error).into()),
        Err(_) => Err(native_error("NATIVE_ERROR", "Rust operation panicked")),
    }
}

#[napi(object)]
pub struct ChunkConfig {
    pub size: u32,
    pub overlap: u32,
}

#[napi(object)]
pub struct TextChunk {
    pub text: String,
    pub byte_start: u32,
    pub byte_end: u32,
    pub token_count: u32,
}

fn number(value: usize) -> zvs_core::Result<u32> {
    u32::try_from(value).map_err(|_| CoreError::InvalidInput("Text exceeds native limits".into()))
}

#[napi]
pub async fn chunk_text(text: String, config: ChunkConfig) -> napi::Result<Vec<TextChunk>> {
    guarded(|| {
        chunk::chunk_text(
            &text,
            chunk::ChunkConfig {
                size: config.size as usize,
                overlap: config.overlap as usize,
            },
            &CancellationToken::new(),
        )?
        .into_iter()
        .map(|chunk| {
            Ok(TextChunk {
                text: chunk.text.to_owned(),
                byte_start: number(chunk.byte_range.start)?,
                byte_end: number(chunk.byte_range.end)?,
                token_count: number(chunk.token_count)?,
            })
        })
        .collect()
    })
}

#[napi]
pub async fn hash_bytes(bytes: Buffer) -> napi::Result<String> {
    guarded(|| hash::content_hash(&bytes, &CancellationToken::new()))
}

#[cfg(feature = "debug-panic")]
#[napi]
pub async fn debug_panic() -> napi::Result<()> {
    guarded(|| panic!("deliberate native panic"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panic_is_an_error_and_subsequent_work_succeeds() {
        let error = guarded::<()>(|| panic!("test panic")).unwrap_err();
        let payload: serde_json::Value = serde_json::from_str(&error.reason).unwrap();
        assert_eq!(payload["code"], "NATIVE_ERROR");
        assert_eq!(guarded(|| Ok(42)).unwrap(), 42);
    }

    #[test]
    fn preserves_core_error_codes() {
        for error in [
            CoreError::InvalidInput("invalid".into()),
            CoreError::Cancelled,
            CoreError::Backend("backend".into()),
            CoreError::Io(std::io::ErrorKind::NotFound.into()),
            CoreError::Io(std::io::ErrorKind::PermissionDenied.into()),
        ] {
            let code = error.code();
            let converted: napi::Error = CoreFailure(error).into();
            let payload: serde_json::Value = serde_json::from_str(&converted.reason).unwrap();
            assert_eq!(payload["code"], code);
        }
    }
}
