#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Oem,
    Ansi,
}

fn incomplete_tail(bytes: &[u8]) -> usize {
    let start = bytes.len().saturating_sub(3);
    for index in (start..bytes.len()).rev() {
        let byte = bytes[index];
        let back = bytes.len() - index;
        if byte & 0b1100_0000 == 0b1000_0000 {
            continue;
        }
        let needed = match byte {
            0b1111_0000..=0b1111_0111 => 4,
            0b1110_0000..=0b1110_1111 => 3,
            0b1100_0000..=0b1101_1111 => 2,
            _ => 1,
        };
        return if needed > back { back } else { 0 };
    }
    0
}

fn score(text: &str) -> i64 {
    text.chars()
        .map(|character| {
            if character.is_alphabetic() && !('\u{2500}'..='\u{259f}').contains(&character) {
                1
            } else if character == '\u{fffd}'
                || ('\u{2500}'..='\u{259f}').contains(&character)
                || (character.is_control() && !character.is_whitespace())
            {
                -2
            } else {
                0
            }
        })
        .sum()
}

#[cfg(windows)]
fn page_text(page: Page, bytes: &[u8]) -> String {
    match page {
        Page::Oem => crate::windows::oem_text(bytes),
        Page::Ansi => crate::windows::ansi_text(bytes),
    }
}

#[cfg(not(windows))]
fn page_text(_page: Page, bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn legacy_line(bytes: &[u8], last: &mut Page) -> String {
    if std::str::from_utf8(bytes).is_ok() {
        return String::from_utf8_lossy(bytes).into_owned();
    }
    let oem = page_text(Page::Oem, bytes);
    let ansi = page_text(Page::Ansi, bytes);
    let (oem_score, ansi_score) = (score(&oem), score(&ansi));
    if ansi_score > oem_score || (ansi_score == oem_score && *last == Page::Ansi) {
        *last = Page::Ansi;
        ansi
    } else {
        *last = Page::Oem;
        oem
    }
}

const LINE_HOLD_MAX: usize = 256;

fn line_break(byte: &u8) -> bool {
    matches!(byte, b'\n' | b'\r')
}

fn legacy_text(bytes: &[u8], last: &mut Page) -> String {
    bytes
        .split_inclusive(line_break)
        .map(|line| legacy_line(line, last))
        .collect()
}

#[derive(Debug, Clone)]
pub struct ConsoleText {
    pending: Vec<u8>,
    last: Page,
}

impl Default for ConsoleText {
    fn default() -> Self {
        Self::new()
    }
}

impl ConsoleText {
    pub const fn new() -> Self {
        Self {
            pending: Vec::new(),
            last: Page::Oem,
        }
    }

    pub fn push(&mut self, bytes: &[u8]) -> String {
        self.pending.extend_from_slice(bytes);
        let keep = incomplete_tail(&self.pending);
        let ready = self.pending.len() - keep;
        if let Ok(text) = std::str::from_utf8(&self.pending[..ready]) {
            let text = text.to_owned();
            self.pending.drain(..ready);
            return text;
        }
        let cut = if self.pending.len() > LINE_HOLD_MAX {
            self.pending.len()
        } else {
            self.pending
                .iter()
                .rposition(line_break)
                .map_or(0, |index| index + 1)
        };
        let rest = self.pending.split_off(cut);
        let bytes = std::mem::replace(&mut self.pending, rest);
        legacy_text(&bytes, &mut self.last)
    }

    pub fn finish(&mut self) -> String {
        let bytes = std::mem::take(&mut self.pending);
        if bytes.is_empty() {
            return String::new();
        }
        match std::str::from_utf8(&bytes) {
            Ok(text) => text.to_owned(),
            Err(_) => legacy_text(&bytes, &mut self.last),
        }
    }
}

pub fn console_text(bytes: &[u8]) -> String {
    let mut decoder = ConsoleText::new();
    let mut text = decoder.push(bytes);
    text.push_str(&decoder.finish());
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_split_between_chunks_is_joined_without_replacement() {
        let bytes = "Привет, мир".as_bytes();
        let mut decoder = ConsoleText::new();
        let mut text = String::new();
        for chunk in bytes.chunks(3) {
            text.push_str(&decoder.push(chunk));
        }
        text.push_str(&decoder.finish());
        assert_eq!(text, "Привет, мир");
    }

    #[test]
    fn a_dangling_utf8_lead_at_the_end_is_not_lost() {
        let mut decoder = ConsoleText::new();
        assert_eq!(decoder.push(b"ok \xd0"), "ok ");
        assert!(!decoder.finish().is_empty());
    }

    #[test]
    fn ascii_and_utf8_pass_through_unchanged() {
        assert_eq!(console_text(b"cargo test\r\n"), "cargo test\r\n");
        assert_eq!(console_text("Файл не найден".as_bytes()), "Файл не найден");
        assert_eq!(console_text(b""), "");
    }

    #[test]
    fn readable_text_outscores_box_drawing_and_replacement_marks() {
        assert!(score("Том в устройстве C") > score("╥хъє∙р ъюфютр"));
        assert!(score("Файл") > score("\u{fffd}\u{fffd}"));
    }

    #[cfg(windows)]
    #[test]
    fn a_hidden_console_mixing_ansi_and_oem_lines_reads_as_russian() {
        if crate::windows::oem_page() != 866 || crate::windows::ansi_page() != 1251 {
            return;
        }
        let captured: &[u8] = &[
            0xd2, 0xe5, 0xea, 0xf3, 0xf9, 0xe0, 0xff, 0x20, 0xea, 0xee, 0xe4, 0xee, 0xe2, 0xe0,
            0xff, 0x20, 0xf1, 0xf2, 0xf0, 0xe0, 0xed, 0xe8, 0xf6, 0xe0, 0x3a, 0x20, 0x38, 0x36,
            0x36, 0x0d, 0x0a, 0x20, 0x92, 0xae, 0xac, 0x20, 0xa2, 0x20, 0xe3, 0xe1, 0xe2, 0xe0,
            0xae, 0xa9, 0xe1, 0xe2, 0xa2, 0xa5, 0x20, 0x43, 0x0d, 0x0a,
        ];
        let mut decoder = ConsoleText::new();
        let mut text = String::new();
        for chunk in captured.chunks(7) {
            text.push_str(&decoder.push(chunk));
        }
        text.push_str(&decoder.finish());
        assert_eq!(
            text,
            "Текущая кодовая страница: 866\r\n Том в устройстве C\r\n"
        );
    }

    #[cfg(windows)]
    #[test]
    fn console_code_page_output_is_decoded_as_letters() {
        let cyrillic = |bytes: &[u8]| !bytes.is_empty() && !bytes.contains(&b'?');
        let oem = crate::windows::encode(crate::windows::oem_page(), "Файл не найден");
        if cyrillic(&oem) {
            assert_eq!(console_text(&oem), "Файл не найден");
        }
        let ansi = crate::windows::encode(crate::windows::ansi_page(), "Текущая кодовая страница");
        if cyrillic(&ansi) {
            assert_eq!(console_text(&ansi), "Текущая кодовая страница");
        }
    }
}
