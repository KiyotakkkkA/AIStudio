use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use sha2::{Digest, Sha256};

use crate::protocol::{PLATFORM, SIGNATURE_VERSION};

pub const SIGNING_SECRET: &str = "key-@@@@)))()((9))-xxxx&&&%%%%%";
pub const SIGNING_WINDOW_MS: i64 = 5 * 60 * 1000;

const BLOCK: usize = 64;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn hmac_sha256_hex(key: &[u8], message: &[u8]) -> String {
    let mut block = [0u8; BLOCK];
    if key.len() > BLOCK {
        block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let pad = |byte: u8| block.map(|value| value ^ byte);
    let inner = Sha256::new()
        .chain_update(pad(0x36))
        .chain_update(message)
        .finalize();
    let outer = Sha256::new()
        .chain_update(pad(0x5c))
        .chain_update(inner)
        .finalize();
    hex(&outer)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedRequest<'a> {
    pub request_id: &'a str,
    pub user_id: &'a str,
    pub timestamp_ms: i64,
    pub prompt: &'a str,
}

impl SignedRequest<'_> {
    fn sorted_payload(&self) -> String {
        format!(
            "requestId,{},timestamp,{},user_id,{}",
            self.request_id, self.timestamp_ms, self.user_id
        )
    }

    pub fn signature(&self) -> String {
        let window = self.timestamp_ms.div_euclid(SIGNING_WINDOW_MS);
        let key = hmac_sha256_hex(SIGNING_SECRET.as_bytes(), window.to_string().as_bytes());
        let message = format!(
            "{}|{}|{}",
            self.sorted_payload(),
            STANDARD.encode(self.prompt.trim()),
            self.timestamp_ms
        );
        hmac_sha256_hex(key.as_bytes(), message.as_bytes())
    }

    pub fn query(&self, token: &str) -> Vec<(&'static str, String)> {
        let timestamp = self.timestamp_ms.to_string();
        vec![
            ("timestamp", timestamp.clone()),
            ("requestId", self.request_id.to_owned()),
            ("user_id", self.user_id.to_owned()),
            ("version", SIGNATURE_VERSION.to_owned()),
            ("platform", PLATFORM.to_owned()),
            ("token", token.to_owned()),
            ("signature_timestamp", timestamp),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_matches_rfc_4231() {
        assert_eq!(
            hmac_sha256_hex(b"Jefe", b"what do ya want for nothing?"),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
        assert_eq!(
            hmac_sha256_hex(
                &[0xaa; 131],
                b"Test Using Larger Than Block-Size Key - Hash Key First"
            ),
            "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
        );
    }

    #[test]
    fn the_signature_chains_the_window_key_over_payload_prompt_and_time() {
        let request = SignedRequest {
            request_id: "r",
            user_id: "u",
            timestamp_ms: 1_792_867_620_312,
            prompt: "  Привет ",
        };
        let key = hmac_sha256_hex(SIGNING_SECRET.as_bytes(), b"5976225");
        let expected = hmac_sha256_hex(
            key.as_bytes(),
            "requestId,r,timestamp,1792867620312,user_id,u|0J/RgNC40LLQtdGC|1792867620312"
                .as_bytes(),
        );
        assert_eq!(request.signature(), expected);
        assert_eq!(request.signature().len(), 64);
        let moved = SignedRequest {
            timestamp_ms: request.timestamp_ms + SIGNING_WINDOW_MS,
            ..request.clone()
        };
        assert_ne!(moved.signature(), request.signature());
    }

    #[test]
    fn the_query_carries_the_signed_fields_and_the_signature_time() {
        let request = SignedRequest {
            request_id: "r",
            user_id: "u",
            timestamp_ms: 7,
            prompt: "p",
        };
        let query = request.query("t");
        assert_eq!(query[0], ("timestamp", "7".to_owned()));
        assert_eq!(query[5], ("token", "t".to_owned()));
        assert_eq!(query[6], ("signature_timestamp", "7".to_owned()));
    }
}
