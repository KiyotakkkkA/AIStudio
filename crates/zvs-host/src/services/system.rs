use std::time::Duration;

use zvs_contract::{
    AppError, HostEventDraft, RunOutcomeDto, RunOutcomeStatus, StreamId, SystemPingOutput,
    Timestamp,
};

use crate::platform::clock::SharedClock;
use crate::platform::events::EventBus;

pub const DEMO_INTERVAL: Duration = Duration::from_millis(200);

pub struct SystemService {
    clock: SharedClock,
    events: EventBus,
    demo_interval: Duration,
}

impl SystemService {
    pub const fn new(clock: SharedClock, events: EventBus, demo_interval: Duration) -> Self {
        Self {
            clock,
            events,
            demo_interval,
        }
    }

    pub fn ping(&self, sent_at: Timestamp) -> SystemPingOutput {
        let host_time = self.clock.now();
        SystemPingOutput {
            pong: true,
            host_time,
            round_trip_hint: (host_time - sent_at) as f64,
        }
    }

    pub fn demo_stream(&self, steps: u64) -> Result<StreamId, AppError> {
        let stream = self.events.open_stream()?;
        let id = stream.id();
        let interval = self.demo_interval;
        tokio::spawn(async move {
            for done in 1..=steps {
                tokio::time::sleep(interval).await;
                if stream.closed() {
                    return;
                }
                stream.emit(HostEventDraft::Progress { done, total: steps });
            }
            stream.end(RunOutcomeDto {
                status: RunOutcomeStatus::Ok,
                code: None,
                message: None,
            });
        });
        Ok(id)
    }
}
