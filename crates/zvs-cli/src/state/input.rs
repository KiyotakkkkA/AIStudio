#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Input {
    text: String,
    cursor: usize,
    history: Vec<String>,
    browsing: Option<usize>,
    draft: String,
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
        self.text = text.to_owned();
        self.cursor = self.text.len();
        self.browsing = None;
    }

    pub fn clear(&mut self) {
        self.set("");
    }

    pub fn insert(&mut self, value: &str) {
        let value = value.replace("\r\n", "\n").replace('\r', "\n");
        self.text.insert_str(self.cursor, &value);
        self.cursor += value.len();
        self.browsing = None;
    }

    pub fn newline(&mut self) {
        self.insert("\n");
    }

    fn previous_boundary(&self) -> Option<usize> {
        self.text[..self.cursor]
            .char_indices()
            .last()
            .map(|(at, _)| at)
    }

    fn next_boundary(&self) -> Option<usize> {
        self.text[self.cursor..]
            .chars()
            .next()
            .map(|next| self.cursor + next.len_utf8())
    }

    pub fn backspace(&mut self) {
        if let Some(at) = self.previous_boundary() {
            self.text.replace_range(at..self.cursor, "");
            self.cursor = at;
        }
    }

    pub fn delete(&mut self) {
        if let Some(end) = self.next_boundary() {
            self.text.replace_range(self.cursor..end, "");
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
        let text = std::mem::take(&mut self.text);
        self.cursor = 0;
        self.browsing = None;
        let trimmed = text.trim().to_owned();
        if !trimmed.is_empty() && self.history.last() != Some(&trimmed) {
            self.history.push(trimmed.clone());
        }
        trimmed
    }

    pub fn history_back(&mut self) {
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
