use super::attachments::{Attachment, Token};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Input {
    text: String,
    cursor: usize,
    history: Vec<String>,
    browsing: Option<usize>,
    draft: String,
    tokens: Vec<Token>,
}

impl Input {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub const fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
    }

    pub fn set(&mut self, text: &str) {
        self.tokens.clear();
        self.text = text.to_owned();
        self.cursor = self.text.len();
        self.browsing = None;
    }

    pub fn clear(&mut self) {
        self.set("");
    }

    pub fn insert(&mut self, value: &str) {
        let value = value.replace("\r\n", "\n").replace('\r', "\n");
        for token in &mut self.tokens {
            if token.start >= self.cursor {
                token.start += value.len();
                token.end += value.len();
            }
        }
        self.text.insert_str(self.cursor, &value);
        self.cursor += value.len();
        self.browsing = None;
    }

    pub fn newline(&mut self) {
        self.insert("\n");
    }

    pub fn attachments(&self) -> Vec<Attachment> {
        self.tokens
            .iter()
            .map(|token| token.attachment.clone())
            .collect()
    }

    pub fn has_images(&self) -> bool {
        self.tokens.iter().any(|t| t.attachment.image())
    }

    pub fn pieces(&self, start: usize, end: usize) -> Vec<(&str, Option<&Attachment>)> {
        let mut pieces = Vec::new();
        let mut at = start;
        for token in self
            .tokens
            .iter()
            .filter(|token| token.start >= start && token.end <= end)
        {
            if at < token.start {
                pieces.push((&self.text[at..token.start], None));
            }
            pieces.push((&self.text[token.start..token.end], Some(&token.attachment)));
            at = token.end;
        }
        if at < end {
            pieces.push((&self.text[at..end], None));
        }
        pieces
    }

    pub fn mention(&self) -> Option<(usize, &str)> {
        let before = &self.text[..self.cursor];
        let at = before.rfind('@')?;
        if at > 0 && !before[..at].ends_with(char::is_whitespace) {
            return None;
        }
        if self.tokens.iter().any(|t| at >= t.start && at < t.end) {
            return None;
        }
        let query = &before[at + 1..];
        (!query.contains('\n')).then_some((at, query))
    }

    pub fn attach(&mut self, attachment: Attachment, start: usize) {
        self.remove(start, self.cursor);
        self.cursor = start;
        let label = format!(
            "[{} {}]",
            attachment.icon(),
            attachment.name().replace(['\n', '\r'], " ")
        );
        self.insert(&label);
        self.tokens.push(Token {
            start,
            end: self.cursor,
            attachment,
        });
        self.tokens.sort_by_key(|t| t.start);
        if self.text[self.cursor..].starts_with(' ') {
            self.right();
        } else {
            self.insert(" ");
        }
    }

    fn remove(&mut self, start: usize, end: usize) {
        self.tokens.retain(|t| t.end <= start || t.start >= end);
        for t in &mut self.tokens {
            if t.start >= end {
                t.start -= end - start;
                t.end -= end - start;
            }
        }
        self.text.replace_range(start..end, "");
    }

    pub fn plain_text(&self) -> String {
        let mut text = self.text.clone();
        for token in self.tokens.iter().rev() {
            text.replace_range(token.start..token.end, "");
        }
        text.trim().to_owned()
    }

    fn previous_boundary(&self) -> Option<usize> {
        if let Some(token) = self.tokens.iter().find(|t| t.end == self.cursor) {
            return Some(token.start);
        }
        self.text[..self.cursor]
            .char_indices()
            .last()
            .map(|(at, _)| at)
    }

    fn next_boundary(&self) -> Option<usize> {
        if let Some(token) = self.tokens.iter().find(|t| t.start == self.cursor) {
            return Some(token.end);
        }
        self.text[self.cursor..]
            .chars()
            .next()
            .map(|next| self.cursor + next.len_utf8())
    }

    pub fn backspace(&mut self) {
        if let Some(at) = self.previous_boundary() {
            self.remove(at, self.cursor);
            self.cursor = at;
        }
    }

    pub fn delete(&mut self) {
        if let Some(end) = self.next_boundary() {
            self.remove(self.cursor, end);
        }
    }

    pub fn left(&mut self) {
        if let Some(at) = self.previous_boundary() {
            self.cursor = at;
        }
    }

    pub fn right(&mut self) {
        if let Some(at) = self.next_boundary() {
            self.cursor = at;
        }
    }

    fn line_start(&self) -> usize {
        self.text[..self.cursor].rfind('\n').map_or(0, |at| at + 1)
    }

    fn line_end(&self) -> usize {
        self.text[self.cursor..]
            .find('\n')
            .map_or(self.text.len(), |at| self.cursor + at)
    }

    pub fn home(&mut self) {
        self.cursor = self.line_start();
    }

    pub fn end(&mut self) {
        self.cursor = self.line_end();
    }

    pub fn on_first_line(&self) -> bool {
        !self.text[..self.cursor].contains('\n')
    }

    pub fn on_last_line(&self) -> bool {
        !self.text[self.cursor..].contains('\n')
    }

    fn column(&self) -> usize {
        self.text[self.line_start()..self.cursor].chars().count()
    }

    fn place(&mut self, start: usize, column: usize) {
        let line = &self.text[start..];
        let end = line.find('\n').unwrap_or(line.len());
        let offset = line[..end]
            .char_indices()
            .nth(column)
            .map_or(end, |(at, _)| at);
        self.cursor = start + offset;
        if let Some(token) = self
            .tokens
            .iter()
            .find(|t| t.start < self.cursor && self.cursor < t.end)
        {
            self.cursor = token.end;
        }
    }

    pub fn up(&mut self) -> bool {
        if self.on_first_line() {
            return false;
        }
        let column = self.column();
        let start = self.line_start();
        let previous = self.text[..start - 1].rfind('\n').map_or(0, |at| at + 1);
        self.place(previous, column);
        true
    }

    pub fn down(&mut self) -> bool {
        if self.on_last_line() {
            return false;
        }
        let column = self.column();
        let next = self.line_end() + 1;
        self.place(next, column);
        true
    }

    pub fn take(&mut self) -> String {
        let had_attachments = !self.tokens.is_empty();
        self.tokens.clear();
        let text = std::mem::take(&mut self.text);
        self.cursor = 0;
        self.browsing = None;
        let trimmed = text.trim().to_owned();
        if !had_attachments && !trimmed.is_empty() && self.history.last() != Some(&trimmed) {
            self.history.push(trimmed.clone());
        }
        trimmed
    }

    pub fn history_back(&mut self) {
        if !self.tokens.is_empty() {
            return;
        }
        if self.history.is_empty() {
            return;
        }
        let index = match self.browsing {
            None => {
                self.draft = self.text.clone();
                self.history.len() - 1
            }
            Some(0) => 0,
            Some(index) => index - 1,
        };
        self.browsing = Some(index);
        self.text = self.history[index].clone();
        self.cursor = self.text.len();
    }

    pub fn history_forward(&mut self) {
        if !self.tokens.is_empty() {
            return;
        }
        let Some(index) = self.browsing else {
            return;
        };
        if index + 1 < self.history.len() {
            self.browsing = Some(index + 1);
            self.text = self.history[index + 1].clone();
        } else {
            self.browsing = None;
            self.text = std::mem::take(&mut self.draft);
        }
        self.cursor = self.text.len();
    }

    pub fn lines_before_cursor(&self) -> (usize, usize) {
        let before = &self.text[..self.cursor];
        let row = before.matches('\n').count();
        let column = before.rsplit('\n').next().unwrap_or_default();
        (row, unicode_width::UnicodeWidthStr::width(column))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image() -> Attachment {
        Attachment::Image {
            path: "C:/test/снимок.png".to_owned(),
            name: "снимок.png".to_owned(),
        }
    }

    #[test]
    fn attachments_are_atomic_and_offsets_survive_surrounding_edits() {
        let mut input = Input::default();
        input.insert("текст @сни");
        input.attach(image(), input.mention().unwrap().0);
        input.left();
        let after = input.cursor();
        input.left();
        assert_eq!(input.cursor(), "текст ".len());
        input.right();
        assert_eq!(input.cursor(), after);
        input.home();
        input.insert("до ");
        input.end();
        input.insert("после");
        assert_eq!(input.plain_text(), "до текст  после");
        for _ in 0..6 {
            input.left();
        }
        input.backspace();
        assert!(input.attachments().is_empty());
        assert_eq!(input.text(), "до текст  после");
        input.attach(image(), input.cursor());
        input.left();
        input.left();
        input.delete();
        assert!(input.attachments().is_empty());
    }

    #[test]
    fn vertical_navigation_never_places_the_cursor_inside_a_token() {
        let mut input = Input::default();
        input.attach(image(), 0);
        input.insert("\nabc");
        input.up();
        assert_eq!(input.cursor(), "[▧ снимок.png]".len());
        input.backspace();
        assert_eq!(input.text(), " \nabc");
    }

    #[test]
    fn editing_respects_multibyte_characters() {
        let mut input = Input::default();
        input.insert("привет");
        input.left();
        input.backspace();
        assert_eq!(input.text(), "привт");
        input.delete();
        assert_eq!(input.text(), "прив");
        input.home();
        input.insert("«");
        input.end();
        input.insert("»");
        assert_eq!(input.text(), "«прив»");
    }

    #[test]
    fn up_and_down_move_between_lines_and_keep_the_column() {
        let mut input = Input::default();
        input.insert("abcdef");
        input.newline();
        input.insert("xy");
        assert!(input.up());
        assert_eq!(input.cursor(), 2);
        assert!(!input.up());
        assert!(input.down());
        assert_eq!(input.cursor(), input.text().len());
        assert!(!input.down());
    }

    #[test]
    fn history_walks_back_and_restores_the_draft() {
        let mut input = Input::default();
        input.insert("first");
        assert_eq!(input.take(), "first");
        input.insert("second");
        input.take();
        input.insert("draft");
        input.history_back();
        assert_eq!(input.text(), "second");
        input.history_back();
        input.history_back();
        assert_eq!(input.text(), "first");
        input.history_forward();
        input.history_forward();
        assert_eq!(input.text(), "draft");
    }

    #[test]
    fn pasted_windows_newlines_become_plain_newlines() {
        let mut input = Input::default();
        input.insert("a\r\nb\rc");
        assert_eq!(input.text(), "a\nb\nc");
        assert_eq!(input.lines_before_cursor(), (2, 1));
    }
}
