use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use zvs_contract::Timestamp;

pub const CLOCK_ORIGIN: Timestamp = 1_700_000_000_000;

pub trait Clock: Send + Sync {
    fn now(&self) -> Timestamp;
}

pub type SharedClock = Arc<dyn Clock>;

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| {
                Timestamp::try_from(elapsed.as_millis()).unwrap_or(Timestamp::MAX)
            })
    }
}

#[derive(Debug)]
pub struct FakeClock(AtomicI64);

impl FakeClock {
    pub const fn new(start: Timestamp) -> Self {
        Self(AtomicI64::new(start))
    }

    pub fn advance(&self, ms: Timestamp) -> Timestamp {
        self.0.fetch_add(ms, Ordering::SeqCst) + ms
    }

    pub fn set(&self, ms: Timestamp) -> Timestamp {
        self.0.store(ms, Ordering::SeqCst);
        ms
    }
}

impl Default for FakeClock {
    fn default() -> Self {
        Self::new(CLOCK_ORIGIN)
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Timestamp {
        self.0.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fake_clock_starts_at_the_origin_and_moves_on_request() {
        let clock = FakeClock::default();
        assert_eq!(clock.now(), CLOCK_ORIGIN);
        assert_eq!(clock.advance(5), CLOCK_ORIGIN + 5);
        assert_eq!(clock.now(), CLOCK_ORIGIN + 5);
        assert_eq!(clock.set(10), 10);
        assert_eq!(clock.now(), 10);
    }

    #[test]
    fn the_system_clock_reads_epoch_milliseconds() {
        assert!(SystemClock.now() > CLOCK_ORIGIN);
    }
}
