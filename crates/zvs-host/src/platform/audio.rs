use std::sync::Arc;

pub const SAMPLE_RATE: u32 = 16_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

pub trait Recording: Send {
    fn finish(self: Box<Self>) -> Result<Vec<i16>, String>;
}

pub trait AudioInput: Send + Sync {
    fn capture_devices(&self) -> Result<Vec<CaptureDevice>, String>;

    fn record(&self, device_id: Option<&str>) -> Result<Box<dyn Recording>, String>;
}

pub type SharedAudioInput = Arc<dyn AudioInput>;

#[derive(Debug, Clone, Copy, Default)]
pub struct NoAudioInput;

impl AudioInput for NoAudioInput {
    fn capture_devices(&self) -> Result<Vec<CaptureDevice>, String> {
        Ok(Vec::new())
    }

    fn record(&self, _device_id: Option<&str>) -> Result<Box<dyn Recording>, String> {
        Err("audio capture is not available on this platform".to_owned())
    }
}
