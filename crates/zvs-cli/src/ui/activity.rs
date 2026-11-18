use ratatui::text::{Line, Span};
use zvs_contract::ChatActivityDto;

use super::text::wrap;
use super::theme;
use crate::{copy, format};

const DAY_MS: i64 = 86_400_000;
const LEVELS: [&str; 5] = ["·", "░", "▒", "▓", "█"];

pub(super) fn lines(activity: &ChatActivityDto, width: usize) -> Vec<Line<'static>> {
    let days = &activity.daily_tokens;
    let offset = (activity.first_day / DAY_MS + 3).rem_euclid(7) as usize;
    let weeks = (offset + days.len()).div_ceil(7);
    let show_weekdays = width >= 12;
    let chart_width = width - if show_weekdays { 3 } else { 0 };
    let first_week = weeks.saturating_sub(chart_width);
    let peak = days.iter().copied().max().unwrap_or(0);
    let mut months = vec![' '; weeks - first_week];
    let mut previous_month = None;
    let mut next_label = 0;
    for week in first_week..weeks {
        let index = (week * 7).saturating_sub(offset);
        let date = activity.first_day + index as i64 * DAY_MS;
        let (_, month, _) = format::date_parts(date);
        let column = week - first_week;
        if previous_month != Some(month) && column >= next_label && column + 3 <= months.len() {
            let name = copy::ACTIVITY_MONTHS[month as usize - 1];
            for (position, character) in name.chars().enumerate() {
                months[column + position] = character;
            }
            next_label = column + 4;
        }
        previous_month = Some(month);
    }
    let month_labels = months.into_iter().collect::<String>();
    let month_indent = if show_weekdays { "   " } else { "" };
    let mut lines = vec![Line::from(Span::styled(
        format!("{month_indent}{month_labels}"),
        theme::muted(),
    ))];
    for (day, label) in copy::ACTIVITY_DAY_LABELS.iter().enumerate() {
        let weekday = show_weekdays.then(|| Span::styled(*label, theme::muted()));
        let cells = (first_week..weeks).map(|week| {
            let tokens = (week * 7 + day)
                .checked_sub(offset)
                .and_then(|index| days.get(index));
            match tokens {
                None => Span::raw(" "),
                Some(0) => Span::styled(LEVELS[0], theme::faint()),
                Some(&tokens) => {
                    let level = (tokens * 4).div_ceil(peak) as usize;
                    Span::styled(LEVELS[level], theme::accent())
                }
            }
        });
        lines.push(Line::from_iter(weekday.into_iter().chain(cells)));
    }
    let legend = format!(
        "{} {} {}",
        copy::ACTIVITY_LESS,
        LEVELS.join(" "),
        copy::ACTIVITY_MORE
    );
    lines.extend(
        wrap(&legend, width)
            .into_iter()
            .map(|line| Line::from(Span::styled(line, theme::muted()))),
    );
    lines.push(Line::default());
    for (label, value) in [
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
            copy::ACTIVITY_LONGEST_STREAK,
            days_label(activity.longest_streak),
        ),
        (
            copy::ACTIVITY_CURRENT_STREAK,
            days_label(activity.current_streak),
        ),
    ] {
        let metric = format!("{label} · {value}");
        lines.extend(
            wrap(&metric, width)
                .into_iter()
                .map(|line| Line::from(Span::styled(line, theme::text()))),
        );
    }
    lines
}

fn days_label(count: u64) -> String {
    format::plural(count, "день", "дня", "дней")
}
