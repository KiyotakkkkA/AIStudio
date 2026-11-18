use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState};

use super::theme;

pub(super) fn draw(
    frame: &mut Frame<'_>,
    area: Rect,
    total: usize,
    offset: usize,
    transparent: bool,
) {
    let visible = usize::from(area.height);
    if area.width < 2 || visible < 2 || total <= visible {
        return;
    }
    let bar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .begin_symbol(None)
        .end_symbol(None)
        .track_symbol(if transparent { None } else { Some("│") })
        .track_style(theme::faint())
        .thumb_style(theme::muted());
    let mut state = ScrollbarState::new(total - visible + 1)
        .viewport_content_length(visible)
        .position(offset);
    frame.render_stateful_widget(bar, area, &mut state);
}
