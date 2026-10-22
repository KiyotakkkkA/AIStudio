use std::ops::Range;

use encoding_rs::{UTF_16BE, UTF_16LE, WINDOWS_1251};
use zvs_contract::DocumentTextEncoding;

const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];
const UTF16LE_BOM: &[u8] = &[0xFF, 0xFE];
const UTF16BE_BOM: &[u8] = &[0xFE, 0xFF];
const PROBE: usize = 8192;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedText {
    pub text: String,
    pub encoding: DocumentTextEncoding,
}

const fn is_control(byte: u8) -> bool {
    matches!(byte, 0x00..=0x08 | 0x0E..=0x1A | 0x1C..=0x1F)
}

pub fn looks_textual(bytes: &[u8]) -> bool {
    if bytes.starts_with(UTF16LE_BOM) || bytes.starts_with(UTF16BE_BOM) {
        return bytes.len().is_multiple_of(2);
    }
    let body = bytes.strip_prefix(UTF8_BOM).unwrap_or(bytes);
    !body.iter().take(PROBE).any(|byte| is_control(*byte))
}

pub fn decode(bytes: &[u8]) -> DecodedText {
    let (text, encoding) = if let Some(body) = bytes.strip_prefix(UTF16LE_BOM) {
        (
            UTF_16LE.decode_without_bom_handling(body).0.into_owned(),
            DocumentTextEncoding::Utf16Le,
        )
    } else if let Some(body) = bytes.strip_prefix(UTF16BE_BOM) {
        (
            UTF_16BE.decode_without_bom_handling(body).0.into_owned(),
            DocumentTextEncoding::Utf16Be,
        )
    } else if let Some(body) = bytes.strip_prefix(UTF8_BOM) {
        (
            String::from_utf8_lossy(body).into_owned(),
            DocumentTextEncoding::Utf8,
        )
    } else if let Ok(text) = std::str::from_utf8(bytes) {
        (text.to_owned(), DocumentTextEncoding::Utf8)
    } else {
        (
            WINDOWS_1251
                .decode_without_bom_handling(bytes)
                .0
                .into_owned(),
            DocumentTextEncoding::Windows1251,
        )
    };
    DecodedText {
        text: text.replace("\r\n", "\n"),
        encoding,
    }
}

fn next_boundary(text: &str, from: usize) -> usize {
    (from + 1..=text.len())
        .find(|index| text.is_char_boundary(*index))
        .unwrap_or(text.len())
}

pub fn paginate(text: &str, max_bytes: usize) -> Vec<Range<usize>> {
    let mut pages = Vec::new();
    let mut start = 0;
    while text.len() - start > max_bytes {
        let mut limit = start + max_bytes;
        while !text.is_char_boundary(limit) {
            limit -= 1;
        }
        let mut end = text[start..limit]
            .rfind('\n')
            .map_or(limit, |at| start + at + 1);
        if end == start {
            end = next_boundary(text, start);
        }
        pages.push(start..end);
        start = end;
    }
    pages.push(start..text.len());
    pages
}

pub fn first_line(text: &str, start: usize) -> u64 {
    let breaks = text[..start].bytes().filter(|byte| *byte == b'\n').count();
    u64::try_from(breaks).unwrap_or(u64::MAX).saturating_add(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_the_encoding_it_decodes_with() {
        assert_eq!(
            decode("Привет\r\nмир".as_bytes()),
            DecodedText {
                text: "Привет\nмир".to_owned(),
                encoding: DocumentTextEncoding::Utf8,
            }
        );
        let cp1251 = [0xCF, 0xF0, 0xE8, 0xE2, 0xE5, 0xF2];
        assert_eq!(
            decode(&cp1251),
            DecodedText {
                text: "Привет".to_owned(),
                encoding: DocumentTextEncoding::Windows1251,
            }
        );
        let utf16: Vec<u8> = UTF16LE_BOM
            .iter()
            .copied()
            .chain("ok".encode_utf16().flat_map(u16::to_le_bytes))
            .collect();
        assert!(looks_textual(&utf16));
        assert_eq!(decode(&utf16).encoding, DocumentTextEncoding::Utf16Le);
        assert_eq!(decode(&utf16).text, "ok");
    }

    #[test]
    fn control_bytes_mark_a_file_as_binary() {
        assert!(looks_textual(b"line\tone\r\n"));
        assert!(!looks_textual(b"PK\x03\x04\x14\0"));
    }

    #[test]
    fn pages_break_on_lines_and_never_inside_a_character() {
        let text = "alpha\nbeta\ngamma\n";
        let pages = paginate(text, 8);
        assert_eq!(
            pages
                .iter()
                .map(|range| &text[range.clone()])
                .collect::<Vec<_>>(),
            ["alpha\n", "beta\n", "gamma\n"]
        );
        assert_eq!(first_line(text, pages[2].start), 3);
        let long = "жжжжжж";
        let pages = paginate(long, 5);
        assert!(pages.iter().all(|range| long.is_char_boundary(range.start)));
        assert_eq!(
            pages
                .iter()
                .map(|range| &long[range.clone()])
                .collect::<String>(),
            long
        );
        let empty = paginate("", 8);
        assert_eq!(empty.len(), 1);
        assert!(empty[0].is_empty());
    }
}
