use zvs_contract::{ProviderStatus, RotationStatus, Timestamp};

use super::probe_outcome::ProbeOutcome;
use super::secret_policy::{DAY_MS, DUE_SOON_WINDOW_MS, rotation_status_within};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialSource {
    Secret,
    Account,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CredentialLifetime {
    pub source: CredentialSource,
    pub expires_at: Option<Timestamp>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusDerivation {
    pub status: ProviderStatus,
    pub detail: Option<String>,
}

impl StatusDerivation {
    fn new(status: ProviderStatus, detail: impl Into<String>) -> Self {
        Self {
            status,
            detail: Some(detail.into()),
        }
    }
}

pub fn derive_status(
    outcome: &ProbeOutcome,
    lifetimes: &[CredentialLifetime],
    now: Timestamp,
) -> StatusDerivation {
    derive_status_within(outcome, lifetimes, now, DUE_SOON_WINDOW_MS)
}

pub fn derive_status_within(
    outcome: &ProbeOutcome,
    lifetimes: &[CredentialLifetime],
    now: Timestamp,
    window: Timestamp,
) -> StatusDerivation {
    if let Some(failure) = probe_failure(outcome) {
        return failure;
    }
    lifetimes
        .iter()
        .find_map(|lifetime| lifetime_warning(lifetime, now, window))
        .unwrap_or(StatusDerivation {
            status: ProviderStatus::Ok,
            detail: None,
        })
}

fn probe_failure(outcome: &ProbeOutcome) -> Option<StatusDerivation> {
    let failed = |detail: &str| Some(StatusDerivation::new(ProviderStatus::Failed, detail));
    match outcome {
        ProbeOutcome::Ok { .. } | ProbeOutcome::OkEmpty { .. } => None,
        ProbeOutcome::Unreachable { detail } | ProbeOutcome::Error { detail, .. } => failed(detail),
        ProbeOutcome::AuthFailed => failed("Ключ отклонён — исправьте учётные данные"),
        ProbeOutcome::AccountNotLinked => failed("Аккаунт не привязан"),
        ProbeOutcome::SessionExpired => failed("Сессия истекла — войдите заново"),
        ProbeOutcome::RateLimited { .. } => failed("Провайдер ограничил частоту запросов"),
    }
}

fn lifetime_warning(
    lifetime: &CredentialLifetime,
    now: Timestamp,
    window: Timestamp,
) -> Option<StatusDerivation> {
    let degraded = |detail: String| Some(StatusDerivation::new(ProviderStatus::Degraded, detail));
    match rotation_status_within(lifetime.expires_at, now, window) {
        RotationStatus::Ok => None,
        RotationStatus::Overdue => degraded(
            match lifetime.source {
                CredentialSource::Secret => "Учётные данные просрочены",
                CredentialSource::Account => "Сессия просрочена — войдите заново",
            }
            .to_owned(),
        ),
        RotationStatus::DueSoon => {
            let days = days_until(lifetime.expires_at, now);
            degraded(match lifetime.source {
                CredentialSource::Secret => format!("Истекает через {days} дн."),
                CredentialSource::Account => format!("Сессия истекает через {days} дн."),
            })
        }
    }
}

pub fn days_until(expires_at: Option<Timestamp>, now: Timestamp) -> i64 {
    expires_at.map_or(0, |expires_at| {
        let remaining = expires_at.saturating_sub(now);
        if remaining <= 0 {
            0
        } else {
            (remaining + DAY_MS - 1) / DAY_MS
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::clock::CLOCK_ORIGIN;

    const NOW: Timestamp = CLOCK_ORIGIN;

    #[test]
    fn credential_lifetime_warnings_are_pure_and_auth_failures_have_distinct_actions() {
        let derive = |outcome: &ProbeOutcome| derive_status(outcome, &[], NOW);
        assert_ne!(
            derive(&ProbeOutcome::AuthFailed).detail,
            derive(&ProbeOutcome::SessionExpired).detail
        );
        for source in [CredentialSource::Secret, CredentialSource::Account] {
            for days in [-1, 2] {
                let result = derive_status(
                    &ProbeOutcome::OkEmpty { latency_ms: 1 },
                    &[CredentialLifetime {
                        source,
                        expires_at: Some(NOW + days * DAY_MS),
                    }],
                    NOW,
                );
                assert_eq!(result.status, ProviderStatus::Degraded);
                assert!(result.detail.is_some_and(|detail| !detail.is_empty()));
            }
        }
    }

    #[test]
    fn every_failure_kind_fails_with_its_copy_and_success_is_ok_without_detail() {
        let detail = |outcome: ProbeOutcome| {
            let derived = derive_status(&outcome, &[], NOW);
            assert_eq!(derived.status, ProviderStatus::Failed);
            derived.detail.expect("a failure carries a detail")
        };
        assert_eq!(
            detail(ProbeOutcome::AccountNotLinked),
            "Аккаунт не привязан"
        );
        assert_eq!(
            detail(ProbeOutcome::RateLimited { retry_after: None }),
            "Провайдер ограничил частоту запросов"
        );
        assert_eq!(
            detail(ProbeOutcome::Unreachable {
                detail: "down".to_owned()
            }),
            "down"
        );
        assert_eq!(
            derive_status(&ProbeOutcome::OkEmpty { latency_ms: 0 }, &[], NOW),
            StatusDerivation {
                status: ProviderStatus::Ok,
                detail: None
            }
        );
    }

    #[test]
    fn the_first_warning_wins_and_days_round_up() {
        let derived = derive_status(
            &ProbeOutcome::OkEmpty { latency_ms: 0 },
            &[
                CredentialLifetime {
                    source: CredentialSource::Secret,
                    expires_at: None,
                },
                CredentialLifetime {
                    source: CredentialSource::Account,
                    expires_at: Some(NOW + DAY_MS + 1),
                },
                CredentialLifetime {
                    source: CredentialSource::Secret,
                    expires_at: Some(NOW - 1),
                },
            ],
            NOW,
        );
        assert_eq!(
            derived.detail.as_deref(),
            Some("Сессия истекает через 2 дн.")
        );
        assert_eq!(days_until(None, NOW), 0);
        assert_eq!(days_until(Some(NOW - DAY_MS), NOW), 0);
        assert_eq!(days_until(Some(NOW + 3 * DAY_MS), NOW), 3);
        assert_eq!(
            derive_status_within(
                &ProbeOutcome::OkEmpty { latency_ms: 0 },
                &[CredentialLifetime {
                    source: CredentialSource::Secret,
                    expires_at: Some(NOW + 3 * DAY_MS),
                }],
                NOW,
                DAY_MS,
            )
            .status,
            ProviderStatus::Ok
        );
    }
}
