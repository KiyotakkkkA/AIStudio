use crate::{CancellationToken, Result, error::check_cancelled};

pub fn content_hash(content: &[u8], cancellation: &CancellationToken) -> Result<String> {
    check_cancelled(cancellation)?;
    let mut hasher = blake3::Hasher::new();
    for block in content.chunks(64 * 1024) {
        check_cancelled(cancellation)?;
        hasher.update(block);
    }
    check_cancelled(cancellation)?;
    Ok(hasher.finalize().to_hex().to_string())
}
