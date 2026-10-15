use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use zvs_contract::{
    AppError, DeviceProfileDto, HostEventDraft, ResourceSampleDto, RunOutcomeDto, RunOutcomeStatus,
    StreamId, SystemNativePingOutput, SystemPingOutput, Timestamp,
};

use crate::drivers::core::{ChunkConfig, InProcessCore, SharedTextCore};
use crate::platform::clock::SharedClock;
use crate::platform::device::{DeviceProbe, DeviceProbeOptions};
use crate::platform::events::EventBus;
use crate::platform::metrics::{ResourceMonitor, ResourceMonitorOptions};

pub const DEMO_INTERVAL: Duration = Duration::from_millis(200);

pub struct SystemService {
    clock: SharedClock,
    events: EventBus,
    demo_interval: Duration,
    core: SharedTextCore,
    device: Arc<DeviceProbe>,
    resources: Arc<ResourceMonitor>,
}

impl SystemService {
    pub fn new(clock: SharedClock, events: EventBus, demo_interval: Duration) -> Self {
        Self {
            core: InProcessCore::shared(),
            device: Arc::new(DeviceProbe::new(DeviceProbeOptions::new(clock.clone()))),
            resources: Arc::new(ResourceMonitor::new(ResourceMonitorOptions::new(
                clock.clone(),
            ))),
            clock,
            events,
            demo_interval,
        }
    }

    #[must_use]
    pub fn with_core(mut self, core: SharedTextCore) -> Self {
        self.core = core;
        self
    }

    #[must_use]
    pub fn with_device(mut self, device: Arc<DeviceProbe>) -> Self {
        self.device = device;
        self
    }

    #[must_use]
    pub fn with_resources(mut self, resources: Arc<ResourceMonitor>) -> Self {
        self.resources = resources;
        self
    }

    pub fn device(&self) -> Arc<DeviceProbe> {
        Arc::clone(&self.device)
    }

    pub fn resources(&self) -> Arc<ResourceMonitor> {
        Arc::clone(&self.resources)
    }

    pub async fn native_ping(&self, text: String) -> Result<SystemNativePingOutput, AppError> {
        let chunks = self.core.chunk(text, ChunkConfig::default()).await?;
        Ok(SystemNativePingOutput {
            count: chunks.len() as u64,
        })
    }

    pub async fn device_profile(&self, refresh: bool) -> Result<DeviceProfileDto, AppError> {
        self.device.profile(refresh).await
    }

    pub async fn resource_sample(&self) -> Result<ResourceSampleDto, AppError> {
        self.resources.sample(&CancellationToken::new()).await
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
