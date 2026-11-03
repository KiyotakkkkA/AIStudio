use super::Key;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Content {
    Help,
    GoalPlan,
    Subagent(String),
    Warning(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Modal {
    pub content: Content,
    pub offset: usize,
}

impl Modal {
    pub const fn new(content: Content) -> Self {
        Self { content, offset: 0 }
    }

    pub fn key(&mut self, key: Key) -> bool {
        match key {
            Key::Esc | Key::Enter => return true,
            Key::Up => self.offset = self.offset.saturating_sub(1),
            Key::Down => self.offset = self.offset.saturating_add(1),
            Key::PageUp => self.offset = self.offset.saturating_sub(super::SCROLL_STEP),
            Key::PageDown => self.offset = self.offset.saturating_add(super::SCROLL_STEP),
            Key::Home => self.offset = 0,
            Key::End => self.offset = usize::MAX,
            _ => {}
        }
        false
    }
}
