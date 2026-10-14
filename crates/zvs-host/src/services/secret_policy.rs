use zvs_contract::{RotationStatus, Timestamp};

pub const HINT_TAIL_LENGTH: usize = 4;
pub const HINT_ELLIPSIS: &str = "…";
pub const DAY_MS: Timestamp = 24 * 60 * 60 * 1000;
pub const DUE_SOON_WINDOW_MS: Timestamp = 14 * DAY_MS;

pub const KNOWN_VALUE_PREFIXES: &[&str] = &[
    "sk-ant-api03-",
    "sk-or-v1-",
    "osk_live_",
    "osk_test_",
    "xoxb-",
    "ghp_",
    "sk-",
];

pub fn compute_hint(value: &str) -> String {
    let units: Vec<u16> = value.encode_utf16().collect();
    let tail = String::from_utf16_lossy(&units[units.len().saturating_sub(HINT_TAIL_LENGTH)..]);
    let prefix = KNOWN_VALUE_PREFIXES
        .iter()
        .find(|candidate| {
            value.starts_with(*candidate) && units.len() >= candidate.len() + HINT_TAIL_LENGTH
        })
        .copied()
        .unwrap_or_default();
    format!("{prefix}{HINT_ELLIPSIS}{tail}")
}

pub fn rotation_status(rotates_at: Option<Timestamp>, now: Timestamp) -> RotationStatus {
    rotation_status_within(rotates_at, now, DUE_SOON_WINDOW_MS)
}

pub fn rotation_status_within(
    rotates_at: Option<Timestamp>,
    now: Timestamp,
    window: Timestamp,
) -> RotationStatus {
    match rotates_at {
        None => RotationStatus::Ok,
        Some(at) if at <= now => RotationStatus::Overdue,
        Some(at) if at - now <= window => RotationStatus::DueSoon,
        Some(_) => RotationStatus::Ok,
    }
}

pub fn rotation_due_at(rotation_days: Option<i64>, from: Timestamp) -> Option<Timestamp> {
    rotation_days
        .filter(|days| *days > 0)
        .map(|days| from.saturating_add(days.saturating_mul(DAY_MS)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::clock::CLOCK_ORIGIN;

    const VALUE: &str = "sk-or-v1-0a1b2c3d4e5f60718293a4b5c6d7e8f9c31";

    #[test]
    fn rotation_status_turns_over_at_fourteen_days_and_at_expiry() {
        let now = CLOCK_ORIGIN;
        assert_eq!(rotation_status(None, now), RotationStatus::Ok);
        assert_eq!(
            rotation_status(Some(now + DUE_SOON_WINDOW_MS + 1), now),
            RotationStatus::Ok
        );
        assert_eq!(
            rotation_status(Some(now + DUE_SOON_WINDOW_MS), now),
            RotationStatus::DueSoon
        );
        assert_eq!(rotation_status(Some(now + 1), now), RotationStatus::DueSoon);
        assert_eq!(rotation_status(Some(now), now), RotationStatus::Overdue);
        assert_eq!(rotation_status(Some(now - 1), now), RotationStatus::Overdue);

        assert_eq!(rotation_due_at(None, now), None);
        assert_eq!(rotation_due_at(Some(0), now), None);
        assert_eq!(rotation_due_at(Some(-3), now), None);
        assert_eq!(rotation_due_at(Some(30), now), Some(now + 30 * DAY_MS));
    }

    #[test]
    fn the_hint_keeps_at_most_four_characters_of_the_value() {
        assert_eq!(compute_hint(VALUE), "sk-or-v1-…9c31");
        assert_eq!(
            compute_hint("sk-ant-api03-aaaaaaaaaaaaaaaa7d10"),
            "sk-ant-api03-…7d10"
        );
        assert_eq!(compute_hint("osk_live_aaaaaaaaaaaa4f2a"), "osk_live_…4f2a");
        assert_eq!(compute_hint("plain-password-9c31"), "…9c31");
        assert_eq!(compute_hint("ab"), "…ab");
        assert_eq!(compute_hint("sk-ab"), "…k-ab");
        assert_eq!(compute_hint("пароль-ёжик"), "…ёжик");

        for value in [VALUE, "osk_live_aaaaaaaaaaaa4f2a", "plain-password-9c31"] {
            let hint = compute_hint(value);
            let tail =
                &hint[hint.find(HINT_ELLIPSIS).expect("an ellipsis") + HINT_ELLIPSIS.len()..];
            assert!(tail.chars().count() <= HINT_TAIL_LENGTH);
            assert!(!hint.contains(value));
        }
    }
}
