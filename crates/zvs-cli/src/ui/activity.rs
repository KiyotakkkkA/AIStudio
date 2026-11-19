use std::ops::Range;

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;
use zvs_contract::ChatActivityDto;

use super::text::wrap;
use super::theme;
use crate::{copy, format};

const DAY_MS: i64 = 86_400_000;
const LEVELS: [&str; 5] = ["·", "░", "▒", "▓", "█"];
const WEEKDAY_WIDTH: usize = 3;
const METRIC_GAP: usize = 3;
const DAY_TOKENS: &str = "Токены по дням";

pub(super) fn lines(activity: &ChatActivityDto, width: usize) -> Vec<Line<'static>> {
    if width == 0 {
        return Vec::new();
    }
    let mut lines = metrics(activity, width);
    if !activity.daily_tokens.is_empty() {
        lines.push(Line::default());
        lines.extend(chart(activity, width));
    }
    lines
}

fn metrics(activity: &ChatActivityDto, width: usize) -> Vec<Line<'static>> {
    let metrics = [
        (copy::ACTIVITY_TOTAL, format::tokens(activity.total_tokens)),
        (
            copy::ACTIVITY_PEAK,
            format::tokens(activity.peak_daily_tokens),
        ),
        (
            copy::ACTIVITY_LONGEST_TURN,
            format::runtime(activity.longest_turn_ms),
        ),
        (
            copy::ACTIVITY_CURRENT_STREAK,
            days_label(activity.current_streak),
        ),
        (
            copy::ACTIVITY_LONGEST_STREAK,
            days_label(activity.longest_streak),
        ),
    ];
    let column_width = metrics
        .iter()
        .map(|(label, value)| label.width().max(value.as_str().width()))
        .max()
        .unwrap_or_default()
        + METRIC_GAP;
    let columns = ((width + METRIC_GAP) / column_width).clamp(1, metrics.len());
    let mut lines = Vec::new();
    if columns == 1 {
        for (label, value) in metrics {
            if label.width() + value.as_str().width() + METRIC_GAP <= width {
                lines.push(Line::from(vec![
                    Span::styled(value, theme::strong()),
                    Span::raw("   "),
                    Span::styled(label, theme::muted()),
                ]));
            } else {
                push_text(&mut lines, &value, width, theme::strong());
                push_text(&mut lines, label, width, theme::muted());
            }
        }
        return lines;
    }

    for (row_index, row) in metrics.chunks(columns).enumerate() {
        if row_index > 0 {
            lines.push(Line::default());
        }
        let mut values = Vec::with_capacity(row.len());
        let mut labels = Vec::with_capacity(row.len());
        for (index, (label, value)) in row.iter().enumerate() {
            let size = if index + 1 == row.len() {
                0
            } else {
                label.width().max(value.as_str().width()) + METRIC_GAP
            };
            values.push(Span::styled(format!("{value:<size$}"), theme::strong()));
            labels.push(Span::styled(format!("{label:<size$}"), theme::muted()));
        }
        lines.push(Line::from(values));
        lines.push(Line::from(labels));
    }
    lines
}

fn chart(activity: &ChatActivityDto, width: usize) -> Vec<Line<'static>> {
    let days = &activity.daily_tokens;
    let offset = (activity.first_day / DAY_MS + 3).rem_euclid(7) as usize;
    let weeks = (offset + days.len()).div_ceil(7);
    let weekday_width = if width >= 12 { WEEKDAY_WIDTH } else { 0 };
    let available = width - weekday_width;
    let stride = if weeks * 2 - 1 <= available { 2 } else { 1 };
    let visible_weeks = weeks.min(available.div_ceil(stride));
    let first_week = weeks - visible_weeks;
    let first_index = (first_week * 7).saturating_sub(offset);
    let first_date = activity.first_day + first_index as i64 * DAY_MS;
    let last_date = activity.first_day + (days.len() - 1) as i64 * DAY_MS;
    let peak = days.iter().copied().max().unwrap_or(0);
    let mut lines = Vec::new();
    push_text(&mut lines, DAY_TOKENS, width, theme::eyebrow());
    push_text(
        &mut lines,
        &format!(
            "{}–{} · UTC",
            format::date(first_date),
            format::date(last_date)
        ),
        width,
        theme::muted(),
    );
    lines.push(Line::default());
    lines.push(month_labels(
        activity.first_day,
        first_week..weeks,
        offset,
        stride,
        weekday_width,
    ));
    let styles = [
        theme::faint(),
        Style::new().fg(theme::ember(1.0)),
        Style::new().fg(theme::ember(0.72)),
        Style::new().fg(theme::ember(0.4)),
        Style::new().fg(theme::ember(0.0)),
    ];
    for (day, label) in copy::ACTIVITY_DAY_LABELS.iter().enumerate() {
        let mut cells = Vec::with_capacity(1 + visible_weeks * stride);
        if weekday_width > 0 {
            cells.push(Span::styled(*label, theme::muted()));
        }
        for week in first_week..weeks {
            if stride == 2 && week > first_week {
                cells.push(Span::raw(" "));
            }
            let tokens = (week * 7 + day)
                .checked_sub(offset)
                .and_then(|index| days.get(index));
            cells.push(match tokens {
                None => Span::raw(" "),
                Some(&tokens) => {
                    let level = if tokens == 0 {
                        0
                    } else {
                        (u128::from(tokens) * 4).div_ceil(u128::from(peak)) as usize
                    };
                    Span::styled(LEVELS[level], styles[level])
                }
            });
        }
        lines.push(Line::from(cells));
    }
    lines.push(Line::default());
    let legend = format!(
        "{} {} {}",
        copy::ACTIVITY_LESS,
        LEVELS.join(" "),
        copy::ACTIVITY_MORE
    );
    if legend.as_str().width() <= width {
        let mut spans = vec![Span::styled(
            format!("{}  ", copy::ACTIVITY_LESS),
            theme::muted(),
        )];
        for (index, glyph) in LEVELS.iter().enumerate() {
            spans.push(Span::styled(*glyph, styles[index]));
        }
        spans.push(Span::styled(
            format!("  {}", copy::ACTIVITY_MORE),
            theme::muted(),
        ));
        lines.push(Line::from(spans));
    } else {
        push_text(&mut lines, &legend, width, theme::muted());
    }
    lines
}

fn month_labels(
    first_day: i64,
    weeks: Range<usize>,
    offset: usize,
    stride: usize,
    weekday_width: usize,
) -> Line<'static> {
    let chart_width = weeks.len() * stride - (stride - 1);
    let mut labels = vec![' '; weekday_width + chart_width];
    let mut previous_month = None;
    let mut next_label = weekday_width;
    for (column, week) in weeks.enumerate() {
        let index = (week * 7).saturating_sub(offset);
        let date = first_day + index as i64 * DAY_MS;
        let (_, month, _) = format::date_parts(date);
        let column = weekday_width + column * stride;
        if previous_month != Some(month) && column >= next_label {
            let name = copy::ACTIVITY_MONTHS[month as usize - 1];
            let length = name.chars().count();
            if column + length <= labels.len() {
                for (position, character) in name.chars().enumerate() {
                    labels[column + position] = character;
                }
                next_label = column + length + 1;
            }
        }
        previous_month = Some(month);
    }
    Line::from(Span::styled(
        labels.into_iter().collect::<String>(),
        theme::muted(),
    ))
}

fn push_text(lines: &mut Vec<Line<'static>>, text: &str, width: usize, style: Style) {
    lines.extend(
        wrap(text, width)
            .into_iter()
            .map(|line| Line::from(Span::styled(line, style))),
    );
}

fn days_label(count: u64) -> String {
    format::plural(count, "день", "дня", "дней")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn activity() -> ChatActivityDto {
        let mut daily_tokens = vec![0; 364];
        daily_tokens[0] = 100;
        daily_tokens[363] = 100;
        ChatActivityDto {
            first_day: 4 * DAY_MS,
            daily_tokens,
            total_tokens: 200,
            peak_daily_tokens: 100,
            longest_turn_ms: 120_000,
            longest_streak: 1,
            current_streak: 1,
        }
    }

    fn cells(lines: &[Line<'_>]) -> usize {
        lines
            .iter()
            .flat_map(|line| &line.spans)
            .filter(|span| LEVELS.contains(&span.content.as_ref()))
            .count()
    }

    #[test]
    fn responsive_chart_keeps_recent_weeks_and_fits_the_available_columns() {
        let activity = activity();
        for width in [1, 12, 40, 55, 80, 120, 180] {
            assert!(
                lines(&activity, width)
                    .iter()
                    .all(|line| line.width() <= width)
            );
        }

        let narrow = chart(&activity, 40);
        let compact = chart(&activity, 80);
        let wide = chart(&activity, 120);
        assert_eq!(cells(&narrow), 37 * 7 + LEVELS.len());
        assert_eq!(cells(&compact), 364 + LEVELS.len());
        assert_eq!(cells(&wide), cells(&compact));
        assert!(wide[4].width() > compact[4].width());
    }

    #[test]
    fn wide_status_starts_with_the_values_then_their_labels() {
        let lines = lines(&activity(), 120);
        assert_eq!(lines[0].spans.len(), 5);
        assert!(lines[0].spans[0].content.starts_with(&format::tokens(200)));
        assert!(lines[1].spans[0].content.starts_with(copy::ACTIVITY_TOTAL));
    }
}
