#![allow(unsafe_code)]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use ::windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use ::windows::Win32::Media::Audio::{
    AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
    AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, DEVICE_STATE_ACTIVE, IAudioCaptureClient,
    IAudioClient, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, WAVE_FORMAT_PCM,
    WAVEFORMATEX, eCapture, eConsole,
};
use ::windows::Win32::System::Com::StructuredStorage::{
    PropVariantClear, PropVariantToStringAlloc,
};
use ::windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
    CoUninitialize, STGM_READ,
};
use ::windows::core::{HSTRING, PWSTR};
use zvs_host::platform::audio::{AudioInput, CaptureDevice, Recording, SAMPLE_RATE};
use zvs_host::voice::MAX_RECORDING_SAMPLES;

const BUFFER_HNS: i64 = 2_000_000;
const POLL: Duration = Duration::from_millis(10);

#[derive(Debug, Clone, Copy, Default)]
pub struct WasapiInput;

struct ComScope(bool);

impl ComScope {
    fn enter() -> Self {
        Self(unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok())
    }
}

impl Drop for ComScope {
    fn drop(&mut self) {
        if self.0 {
            unsafe { CoUninitialize() };
        }
    }
}

fn failure(step: &str, error: &::windows::core::Error) -> String {
    format!("WASAPI {step} failed: {}", error.code())
}

fn take_string(text: PWSTR) -> String {
    let value = unsafe { text.to_string() }.unwrap_or_default();
    unsafe { CoTaskMemFree(Some(text.0.cast_const().cast())) };
    value
}

fn enumerator() -> Result<IMMDeviceEnumerator, String> {
    unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
        .map_err(|error| failure("enumerator", &error))
}

fn device_id(device: &IMMDevice) -> Result<String, String> {
    let id = unsafe { device.GetId() }.map_err(|error| failure("device id", &error))?;
    Ok(take_string(id))
}

fn friendly_name(device: &IMMDevice) -> String {
    let Ok(store) = (unsafe { device.OpenPropertyStore(STGM_READ) }) else {
        return String::new();
    };
    let Ok(mut value) = (unsafe { store.GetValue(&PKEY_Device_FriendlyName) }) else {
        return String::new();
    };
    let name = unsafe { PropVariantToStringAlloc(&value) }
        .map(take_string)
        .unwrap_or_default();
    let _ = unsafe { PropVariantClear(&mut value) };
    name
}

fn capture_format() -> WAVEFORMATEX {
    WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_PCM as u16,
        nChannels: 1,
        nSamplesPerSec: SAMPLE_RATE,
        nAvgBytesPerSec: SAMPLE_RATE * 2,
        nBlockAlign: 2,
        wBitsPerSample: 16,
        cbSize: 0,
    }
}

fn open(device: Option<&str>) -> Result<(IAudioClient, IAudioCaptureClient), String> {
    let enumerator = enumerator()?;
    let endpoint = match device {
        Some(id) => unsafe { enumerator.GetDevice(&HSTRING::from(id)) },
        None => unsafe { enumerator.GetDefaultAudioEndpoint(eCapture, eConsole) },
    }
    .map_err(|error| failure("endpoint", &error))?;
    let client: IAudioClient = unsafe { endpoint.Activate(CLSCTX_ALL, None) }
        .map_err(|error| failure("client", &error))?;
    let format = capture_format();
    unsafe {
        client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
            BUFFER_HNS,
            0,
            &raw const format,
            None,
        )
    }
    .map_err(|error| failure("initialize", &error))?;
    let capture: IAudioCaptureClient =
        unsafe { client.GetService() }.map_err(|error| failure("capture client", &error))?;
    unsafe { client.Start() }.map_err(|error| failure("start", &error))?;
    Ok((client, capture))
}

fn drain(capture: &IAudioCaptureClient, samples: &mut Vec<i16>) -> Result<(), String> {
    loop {
        let pending =
            unsafe { capture.GetNextPacketSize() }.map_err(|error| failure("packet", &error))?;
        if pending == 0 {
            return Ok(());
        }
        let mut data = std::ptr::null_mut();
        let mut frames = 0_u32;
        let mut flags = 0_u32;
        unsafe { capture.GetBuffer(&raw mut data, &raw mut frames, &raw mut flags, None, None) }
            .map_err(|error| failure("buffer", &error))?;
        let count = frames as usize;
        let room = MAX_RECORDING_SAMPLES.saturating_sub(samples.len());
        let silent = flags & AUDCLNT_BUFFERFLAGS_SILENT.0.cast_unsigned() != 0;
        if silent || data.is_null() {
            samples.extend(std::iter::repeat_n(0, count.min(room)));
        } else {
            let frames = unsafe { std::slice::from_raw_parts(data.cast::<i16>(), count) };
            samples.extend_from_slice(&frames[..count.min(room)]);
        }
        unsafe { capture.ReleaseBuffer(frames) }.map_err(|error| failure("release", &error))?;
    }
}

fn capture(
    device: Option<String>,
    stop: &AtomicBool,
    ready: &mpsc::Sender<Result<(), String>>,
) -> Result<Vec<i16>, String> {
    let _com = ComScope::enter();
    let (client, capture) = match open(device.as_deref()) {
        Ok(opened) => {
            let _ = ready.send(Ok(()));
            opened
        }
        Err(reason) => {
            let _ = ready.send(Err(reason.clone()));
            return Err(reason);
        }
    };
    let mut samples = Vec::new();
    let mut outcome = Ok(());
    while !stop.load(Ordering::SeqCst) {
        std::thread::sleep(POLL);
        outcome = drain(&capture, &mut samples);
        if outcome.is_err() {
            break;
        }
    }
    if outcome.is_ok() {
        outcome = drain(&capture, &mut samples);
    }
    let _ = unsafe { client.Stop() };
    outcome.map(|()| samples)
}

struct WasapiRecording {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<Result<Vec<i16>, String>>>,
}

impl Recording for WasapiRecording {
    fn finish(mut self: Box<Self>) -> Result<Vec<i16>, String> {
        self.stop.store(true, Ordering::SeqCst);
        match self.worker.take() {
            Some(worker) => worker
                .join()
                .map_err(|_| "the capture thread panicked".to_owned())?,
            None => Ok(Vec::new()),
        }
    }
}

impl Drop for WasapiRecording {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

impl AudioInput for WasapiInput {
    fn capture_devices(&self) -> Result<Vec<CaptureDevice>, String> {
        let _com = ComScope::enter();
        let enumerator = enumerator()?;
        let default_id = unsafe { enumerator.GetDefaultAudioEndpoint(eCapture, eConsole) }
            .ok()
            .and_then(|device| device_id(&device).ok());
        let collection = unsafe { enumerator.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE) }
            .map_err(|error| failure("endpoint enumeration", &error))?;
        let count =
            unsafe { collection.GetCount() }.map_err(|error| failure("endpoint count", &error))?;
        let mut devices = Vec::new();
        for index in 0..count {
            let device =
                unsafe { collection.Item(index) }.map_err(|error| failure("endpoint", &error))?;
            let id = device_id(&device)?;
            devices.push(CaptureDevice {
                is_default: default_id.as_deref() == Some(id.as_str()),
                name: friendly_name(&device),
                id,
            });
        }
        Ok(devices)
    }

    fn record(&self, device_id: Option<&str>) -> Result<Box<dyn Recording>, String> {
        let stop = Arc::new(AtomicBool::new(false));
        let (ready, started) = mpsc::channel();
        let device = device_id.map(str::to_owned);
        let flag = Arc::clone(&stop);
        let worker = std::thread::Builder::new()
            .name("voice-capture".to_owned())
            .spawn(move || capture(device, &flag, &ready))
            .map_err(|error| format!("capture thread: {error}"))?;
        match started.recv() {
            Ok(Ok(())) => Ok(Box::new(WasapiRecording {
                stop,
                worker: Some(worker),
            })),
            Ok(Err(reason)) => {
                let _ = worker.join();
                Err(reason)
            }
            Err(_) => Err(worker
                .join()
                .map_err(|_| "the capture thread panicked".to_owned())?
                .err()
                .unwrap_or_else(|| "the capture thread stopped".to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_real_capture_endpoints_are_enumerated() {
        let devices = WasapiInput.capture_devices().expect("WASAPI answers");
        assert!(
            devices
                .iter()
                .all(|device| !device.id.is_empty() && !device.name.is_empty())
        );
        assert!(devices.iter().filter(|device| device.is_default).count() <= 1);
    }

    #[test]
    fn the_capture_format_is_sixteen_kilohertz_mono_pcm() {
        let format = capture_format();
        assert_eq!({ format.nSamplesPerSec }, SAMPLE_RATE);
        assert_eq!({ format.nChannels }, 1);
        assert_eq!(
            { format.nAvgBytesPerSec },
            SAMPLE_RATE * u32::from(format.nBlockAlign)
        );
    }

    #[test]
    #[ignore = "opens the real default microphone"]
    fn the_default_microphone_records_a_short_clip() {
        if WasapiInput
            .capture_devices()
            .expect("WASAPI answers")
            .is_empty()
        {
            return;
        }
        let recording = WasapiInput.record(None).expect("capture starts");
        std::thread::sleep(Duration::from_millis(500));
        let samples = recording.finish().expect("capture stops");
        assert!(
            samples.len() > SAMPLE_RATE as usize / 4,
            "{}",
            samples.len()
        );
    }
}
