use serde::{Deserialize, Serialize};

use crate::errors::AppError;
use crate::ids::StreamId;
use crate::primitives::Timestamp;
use crate::validate::{Validate, check, nested, no_refinements};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceTier {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceGpuDto {
    pub vendor: String,
    pub model: String,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub vram_bytes: Option<u64>,
    pub discrete: bool,
}

impl Validate for DeviceGpuDto {
    fn validate(&self) -> Result<(), AppError> {
        check::max_len("vendor", &self.vendor, 200)?;
        check::max_len("model", &self.model, 200)?;
        if let Some(vram) = self.vram_bytes {
            check::safe_integer("vramBytes", vram)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceProfileDto {
    pub platform: String,
    pub arch: String,
    pub cpu_model: String,
    pub cpu_cores: u64,
    pub total_memory_bytes: u64,
    pub free_memory_bytes: u64,
    pub free_disk_bytes: u64,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub gpu: Option<DeviceGpuDto>,
    pub tier: DeviceTier,
    pub measured_at: Timestamp,
}

impl Validate for DeviceProfileDto {
    fn validate(&self) -> Result<(), AppError> {
        check::max_len("platform", &self.platform, 32)?;
        check::max_len("arch", &self.arch, 32)?;
        check::max_len("cpuModel", &self.cpu_model, 200)?;
        check::safe_integer("cpuCores", self.cpu_cores)?;
        check::safe_integer("totalMemoryBytes", self.total_memory_bytes)?;
        check::safe_integer("freeMemoryBytes", self.free_memory_bytes)?;
        check::safe_integer("freeDiskBytes", self.free_disk_bytes)?;
        nested("gpu", self.gpu.validate())?;
        check::timestamp("measuredAt", self.measured_at)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceSampleDto {
    pub at: Timestamp,
    pub cpu_percent: f64,
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
    pub process_memory_bytes: u64,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub gpu_name: Option<String>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub gpu_percent: Option<f64>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub vram_used_bytes: Option<u64>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub vram_total_bytes: Option<u64>,
}

impl Validate for ResourceSampleDto {
    fn validate(&self) -> Result<(), AppError> {
        check::timestamp("at", self.at)?;
        check::number_range("cpuPercent", self.cpu_percent, 0.0, 100.0)?;
        check::safe_integer("memoryUsedBytes", self.memory_used_bytes)?;
        check::safe_integer("memoryTotalBytes", self.memory_total_bytes)?;
        check::safe_integer("processMemoryBytes", self.process_memory_bytes)?;
        if let Some(name) = &self.gpu_name {
            check::max_len("gpuName", name, 200)?;
        }
        if let Some(percent) = self.gpu_percent {
            check::number_range("gpuPercent", percent, 0.0, 100.0)?;
        }
        if let Some(used) = self.vram_used_bytes {
            check::safe_integer("vramUsedBytes", used)?;
        }
        if let Some(total) = self.vram_total_bytes {
            check::safe_integer("vramTotalBytes", total)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemPingInput {
    pub sent_at: Timestamp,
}

impl Validate for SystemPingInput {
    fn validate(&self) -> Result<(), AppError> {
        check::timestamp("sentAt", self.sent_at)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemPingOutput {
    pub pong: bool,
    pub host_time: Timestamp,
    pub round_trip_hint: f64,
}

impl Validate for SystemPingOutput {
    fn validate(&self) -> Result<(), AppError> {
        check::literal_true("pong", self.pong)?;
        check::timestamp("hostTime", self.host_time)?;
        check::finite("roundTripHint", self.round_trip_hint)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemDemoStreamInput {
    pub steps: u64,
}

impl Validate for SystemDemoStreamInput {
    fn validate(&self) -> Result<(), AppError> {
        check::int_range("steps", self.steps, 1, 100)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemDemoStreamOutput {
    pub stream_id: StreamId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemDemoJobInput {
    pub steps: u64,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub interval_ms: Option<u64>,
}

impl Validate for SystemDemoJobInput {
    fn validate(&self) -> Result<(), AppError> {
        check::int_range("steps", self.steps, 1, 1_000)?;
        if let Some(interval) = self.interval_ms {
            check::int_range("intervalMs", interval, 0, 5_000)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemDeviceInput {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub refresh: Option<bool>,
}

impl SystemDeviceInput {
    pub fn refresh(&self) -> bool {
        self.refresh.unwrap_or(false)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemNativePingInput {
    pub text: String,
}

impl Validate for SystemNativePingInput {
    fn validate(&self) -> Result<(), AppError> {
        check::max_len("text", &self.text, 1_000_000)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemNativePingOutput {
    pub count: u64,
}

impl Validate for SystemNativePingOutput {
    fn validate(&self) -> Result<(), AppError> {
        check::safe_integer("count", self.count)
    }
}

no_refinements!(DeviceTier, SystemDemoStreamOutput, SystemDeviceInput);

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_device_profile_keeps_nullable_fields_as_null() {
        let profile = DeviceProfileDto {
            platform: "win32".to_owned(),
            arch: "x64".to_owned(),
            cpu_model: "CPU".to_owned(),
            cpu_cores: 8,
            total_memory_bytes: 16,
            free_memory_bytes: 8,
            free_disk_bytes: 0,
            gpu: None,
            tier: DeviceTier::Medium,
            measured_at: 1,
        };
        assert_eq!(
            serde_json::to_value(&profile).expect("serialises"),
            json!({
                "platform": "win32",
                "arch": "x64",
                "cpuModel": "CPU",
                "cpuCores": 8,
                "totalMemoryBytes": 16,
                "freeMemoryBytes": 8,
                "freeDiskBytes": 0,
                "gpu": null,
                "tier": "medium",
                "measuredAt": 1,
            })
        );
        assert!(profile.validate().is_ok());
    }

    #[test]
    fn device_refresh_defaults_to_false() {
        let input: SystemDeviceInput = serde_json::from_value(json!({})).expect("parses");
        assert!(!input.refresh());
        assert_eq!(serde_json::to_value(&input).expect("serialises"), json!({}));
    }

    #[test]
    fn ping_output_requires_pong_true() {
        let output = SystemPingOutput {
            pong: false,
            host_time: 1,
            round_trip_hint: 0.0,
        };
        assert!(output.validate().is_err());
    }

    #[test]
    fn cpu_percent_stays_within_bounds() {
        let sample = ResourceSampleDto {
            at: 0,
            cpu_percent: 100.5,
            memory_used_bytes: 0,
            memory_total_bytes: 0,
            process_memory_bytes: 0,
            gpu_name: None,
            gpu_percent: None,
            vram_used_bytes: None,
            vram_total_bytes: None,
        };
        assert!(sample.validate().is_err());
    }
}
