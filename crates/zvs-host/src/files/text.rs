use std::fmt::Write as _;

use sha2::{Digest, Sha256};

pub const BOM: &[u8] = &[0xEF, 0xBB, 0xBF];
const BINARY_PROBE: usize = 8192;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TextFormat {
    pub bom: bool,
    pub crlf: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decoded {
    Text { text: String, format: TextFormat },
    Binary,
}

pub fn content_hash(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        let _ = write!(text, "{byte:02x}");
    }
    text
}

pub fn normalize(text: &str) -> String {
    text.replace("\r\n", "\n")
}

pub fn decode(bytes: &[u8]) -> Decoded {
    let (bom, body) = bytes
        .strip_prefix(BOM)
        .map_or((false, bytes), |rest| (true, rest));
    if body.iter().take(BINARY_PROBE).any(|byte| *byte == 0) {
        return Decoded::Binary;
    }
    let Ok(text) = std::str::from_utf8(body) else {
        return Decoded::Binary;
    };
    Decoded::Text {
        text: normalize(text),
        format: TextFormat {
            bom,
            crlf: text.contains("\r\n"),
        },
    }
}

pub fn encode(text: &str, format: TextFormat) -> Vec<u8> {
    let body = if format.crlf {
        text.replace('\n', "\r\n")
    } else {
        text.to_owned()
    };
    let mut bytes = Vec::with_capacity(body.len() + BOM.len());
    if format.bom {
        bytes.extend_from_slice(BOM);
    }
    bytes.extend_from_slice(body.as_bytes());
    bytes
}

pub fn fresh_format(content: &str) -> TextFormat {
    TextFormat {
        bom: false,
        crlf: content.contains("\r\n"),
    }
}
