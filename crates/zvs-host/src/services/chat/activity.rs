use zvs_contract::{ChatActivityDto, MAX_SAFE_INTEGER};

use crate::data::repositories::chat::{ActivityDay, DAY_MS};

const MAX_COUNT: u64 = MAX_SAFE_INTEGER as u64;

pub(super) fn summarize(now: i64, days: Vec<ActivityDay>) -> ChatActivityDto {
    let today = now.max(0) / DAY_MS;
    let year_ago = (today - 364).max(0);
    // Unix day zero was a Thursday. Start the chart on the preceding Monday.
    let first_day = (year_ago - (year_ago + 3).rem_euclid(7)).max(0);
    let mut activity = ChatActivityDto {
        first_day: first_day * DAY_MS,
        daily_tokens: vec![0; (today - first_day + 1) as usize],
        total_tokens: 0,
        peak_daily_tokens: 0,
        longest_turn_ms: 0,
        longest_streak: 0,
        current_streak: 0,
    };
    let mut previous = None;
    let mut streak = 0;
    for day in days {
        let tokens = day.tokens.min(MAX_COUNT);
        activity.total_tokens = (activity.total_tokens + tokens).min(MAX_COUNT);
        activity.peak_daily_tokens = activity.peak_daily_tokens.max(tokens);
        activity.longest_turn_ms = activity
            .longest_turn_ms
            .max(day.longest_turn_ms.min(MAX_COUNT));
        if day.day >= first_day {
            activity.daily_tokens[(day.day - first_day) as usize] = tokens;
        }
        if tokens == 0 {
            continue;
        }
        streak = if previous == Some(day.day - 1) {
            streak + 1
        } else {
            1
        };
        previous = Some(day.day);
        activity.longest_streak = activity.longest_streak.max(streak);
    }
    if previous.is_some_and(|day| day >= today - 1) {
        activity.current_streak = streak;
    }
    activity
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn totals_span_all_history_and_streaks_end_after_a_full_inactive_day() {
        let days = || {
            [
                (1, 100, 60),
                (2, 10, 100),
                (3, 2, 10),
                (499, 2, 10),
                (500, 4, 20),
            ]
            .into_iter()
            .map(|(day, tokens, longest_turn_ms)| ActivityDay {
                day,
                tokens,
                longest_turn_ms,
            })
            .collect()
        };
        let activity = summarize(501 * DAY_MS, days());
        assert_eq!(activity.total_tokens, 118);
        assert_eq!(activity.peak_daily_tokens, 100);
        assert_eq!(activity.longest_turn_ms, 100);
        assert_eq!(activity.longest_streak, 3);
        assert_eq!(activity.current_streak, 2);
        assert_eq!(
            &activity.daily_tokens[activity.daily_tokens.len() - 3..],
            [2, 4, 0]
        );
        assert_eq!((activity.first_day / DAY_MS + 3) % 7, 0);
        assert_eq!(summarize(502 * DAY_MS, days()).current_streak, 0);
    }
}
