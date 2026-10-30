pub mod catalogue;
pub mod whisper;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use async_trait::async_trait;
use serde_json::{Value, json};
use zvs_contract::{
    AppError, AppErrorCode, CatalogueFilter, CatalogueItemState, DownloadItemKind,
    RECORDING_DEVICE_ID_MAX, RECORDING_DEVICE_NAME_MAX, RecordingDeviceDto, SpeechModelDto,
    StartDownloadInput, VOICE_TRANSCRIPT_MAX, VoiceDevicesDto, VoiceModelInput, VoiceModelsDto,
    VoiceSelectInput, VoiceSelectModelInput, VoiceTranscriptDto,
};

use self::catalogue::{
    ENGINE_REF, SPEECH_MODELS, SpeechCatalogueProvider, SpeechModel, recommend, speech_model,
    speech_supported,
};
use self::whisper::{SharedSpeechRecognizer, TranscribeRequest, wav_bytes};
use crate::data::schema::DownloadRow;
use crate::downloads::{CatalogueProvider, DownloadInstaller, DownloadService};
use crate::platform::archive::{ExtractOptions, extract_archive};
use crate::platform::audio::{CaptureDevice, Recording, SAMPLE_RATE, SharedAudioInput};
use crate::platform::gpu::fit_utf16;
use crate::platform::logger::{LogLevel, SharedLogger};
use crate::platform::paths::{DownloadKind, runtime_install_path};
use crate::runtimes::{Platform, RuntimeDevice};
use crate::services::SettingService;

pub const VOICE_DEVICE_SETTING_KEY: &str = "voice.input.device";
pub const VOICE_MODEL_SETTING_KEY: &str = "voice.model";
pub const VOICE_ENGINE_SETTING_KEY: &str = "voice.engine";
pub const ENGINE_RUNTIME_ID: &str = "whisper-cpp";
pub const VOICE_DEVICES_FAILED: &str = "Не удалось получить список устройств записи";
pub const VOICE_DEVICE_NOT_FOUND: &str = "Устройство записи не подключено";
pub const UNNAMED_DEVICE: &str = "Микрофон";
pub const SPEECH_UNSUPPORTED: &str = "Распознавание речи доступно только в Windows на x64";
pub const SPEECH_MODEL_UNKNOWN: &str = "Такой модели распознавания речи нет";
pub const SPEECH_MODEL_NOT_INSTALLED: &str = "Модель распознавания речи не установлена";
pub const SPEECH_NOT_READY: &str =
    "Распознавание речи не настроено: установите модель в параметрах голосового ввода";
pub const ALREADY_RECORDING: &str = "Запись уже идёт";
pub const NOT_RECORDING: &str = "Запись не идёт";
pub const RECORDING_FAILED: &str = "Не удалось начать запись с микрофона";
pub const ENGINE_UNPACK_FAILED: &str = "В архиве движка нет программы whisper-cli";
pub const MAX_RECORDING_SAMPLES: usize = SAMPLE_RATE as usize * 10 * 60;
const MIN_SPEECH_SAMPLES: usize = SAMPLE_RATE as usize / 4;
const MAX_THREADS: usize = 8;

#[derive(Clone)]
pub struct VoiceServiceOptions {
    pub settings: SettingService,
    pub downloads: DownloadService,
    pub audio: SharedAudioInput,
    pub device: Option<Arc<dyn RuntimeDevice>>,
    pub recognizer: SharedSpeechRecognizer,
    pub downloads_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub platform: Platform,
    pub logger: SharedLogger,
}

struct Inner {
    options: VoiceServiceOptions,
    recording: Mutex<Option<Box<dyn Recording>>>,
}

#[derive(Clone)]
pub struct VoiceService {
    inner: Arc<Inner>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn devices_failure(reason: String) -> AppError {
    AppError::new(AppErrorCode::NativeError, VOICE_DEVICES_FAILED).with_detail("reason", reason)
}

fn native(message: &str, reason: impl Into<String>) -> AppError {
    AppError::new(AppErrorCode::NativeError, message).with_detail("reason", reason.into())
}

fn usable_id(id: &str) -> bool {
    !id.is_empty() && id.encode_utf16().count() <= RECORDING_DEVICE_ID_MAX
}

pub fn recording_devices(captured: Vec<CaptureDevice>) -> Vec<RecordingDeviceDto> {
    let mut default_seen = false;
    captured
        .into_iter()
        .filter(|device| usable_id(&device.id))
        .map(|device| {
            let name = fit_utf16(device.name.trim(), RECORDING_DEVICE_NAME_MAX);
            let is_default = device.is_default && !default_seen;
            default_seen |= is_default;
            RecordingDeviceDto {
                id: device.id,
                name: if name.is_empty() {
                    UNNAMED_DEVICE.to_owned()
                } else {
                    name
                },
                is_default,
            }
        })
        .collect()
}

fn missing(state: CatalogueItemState) -> bool {
    matches!(
        state,
        CatalogueItemState::Available | CatalogueItemState::Update
    )
}

fn transcript(text: String) -> VoiceTranscriptDto {
    let text = if text.encode_utf16().count() > VOICE_TRANSCRIPT_MAX {
        fit_utf16(&text, VOICE_TRANSCRIPT_MAX)
    } else {
        text
    };
    VoiceTranscriptDto { text }
}

impl VoiceService {
    pub fn new(options: VoiceServiceOptions) -> Self {
        Self {
            inner: Arc::new(Inner {
                options,
                recording: Mutex::new(None),
            }),
        }
    }

    fn options(&self) -> &VoiceServiceOptions {
        &self.inner.options
    }

    fn log(&self, level: LogLevel, message: &str, fields: &[(&str, Value)]) {
        self.options().logger.log(level, "voice", message, fields);
    }

    pub fn catalogue_provider(&self) -> Arc<dyn CatalogueProvider> {
        Arc::new(SpeechCatalogueProvider {
            platform: self.options().platform,
        })
    }

    fn stored_string(&self, key: &str) -> Result<Option<String>, AppError> {
        Ok(self
            .options()
            .settings
            .get(key)?
            .and_then(|setting| setting.value.as_str().map(str::to_owned))
            .filter(|value| !value.is_empty()))
    }

    async fn capture_devices(&self) -> Result<Vec<RecordingDeviceDto>, AppError> {
        let audio = Arc::clone(&self.options().audio);
        let captured = tokio::task::spawn_blocking(move || audio.capture_devices())
            .await
            .map_err(|error| devices_failure(error.to_string()))?
            .map_err(devices_failure)?;
        Ok(recording_devices(captured))
    }

    fn stored_device(&self) -> Result<Option<String>, AppError> {
        Ok(self
            .stored_string(VOICE_DEVICE_SETTING_KEY)?
            .filter(|id| usable_id(id)))
    }

    pub async fn devices(&self) -> Result<VoiceDevicesDto, AppError> {
        let devices = self.capture_devices().await?;
        Ok(VoiceDevicesDto {
            devices,
            selected_id: self.stored_device()?,
        })
    }

    pub async fn select_device(
        &self,
        input: &VoiceSelectInput,
    ) -> Result<VoiceDevicesDto, AppError> {
        let devices = self.capture_devices().await?;
        if let Some(id) = &input.device_id
            && !devices.iter().any(|device| &device.id == id)
        {
            return Err(
                AppError::new(AppErrorCode::NotFound, VOICE_DEVICE_NOT_FOUND)
                    .with_detail("deviceId", id.clone()),
            );
        }
        let value = input.device_id.as_ref().map_or(Value::Null, |id| json!(id));
        self.options()
            .settings
            .set(VOICE_DEVICE_SETTING_KEY, &value)?;
        Ok(VoiceDevicesDto {
            devices,
            selected_id: input.device_id.clone(),
        })
    }

    async fn states(&self) -> Result<HashMap<String, CatalogueItemState>, AppError> {
        let items = self
            .options()
            .downloads
            .catalogue(&CatalogueFilter {
                kinds: Some(vec![DownloadItemKind::Model, DownloadItemKind::Runtime]),
                ..CatalogueFilter::default()
            })
            .await?;
        Ok(items
            .into_iter()
            .map(|item| (item.item_ref, item.state))
            .collect())
    }

    fn selected_model(&self) -> Result<Option<&'static SpeechModel>, AppError> {
        Ok(self
            .stored_string(VOICE_MODEL_SETTING_KEY)?
            .and_then(|model_ref| speech_model(&model_ref)))
    }

    pub async fn models(&self) -> Result<VoiceModelsDto, AppError> {
        let states = self.states().await?;
        let state_of = |item_ref: &str| {
            states
                .get(item_ref)
                .copied()
                .unwrap_or(CatalogueItemState::Available)
        };
        let profile = match &self.options().device {
            Some(device) => device.profile().await,
            None => None,
        };
        let (recommended, reason) = recommend(profile.as_ref());
        Ok(VoiceModelsDto {
            models: SPEECH_MODELS
                .iter()
                .map(|model| SpeechModelDto {
                    model_ref: model.item_ref.to_owned(),
                    display_name: model.display_name.to_owned(),
                    description: model.description.to_owned(),
                    size_bytes: model.size_bytes,
                    state: state_of(model.item_ref),
                    recommended: model.item_ref == recommended.item_ref,
                })
                .collect(),
            selected_ref: self
                .selected_model()?
                .map(|model| model.item_ref.to_owned()),
            engine_state: state_of(ENGINE_REF),
            recommended_ref: recommended.item_ref.to_owned(),
            reason,
        })
    }

    pub async fn install_model(&self, input: &VoiceModelInput) -> Result<VoiceModelsDto, AppError> {
        if !speech_supported(self.options().platform) {
            return Err(AppError::new(
                AppErrorCode::UnsupportedFormat,
                SPEECH_UNSUPPORTED,
            ));
        }
        let model = speech_model(&input.model_ref).ok_or_else(|| {
            AppError::new(AppErrorCode::NotFound, SPEECH_MODEL_UNKNOWN)
                .with_detail("modelRef", input.model_ref.clone())
        })?;
        let current = self.models().await?;
        let downloads = &self.options().downloads;
        if missing(current.engine_state) && !self.engine_ready() {
            downloads
                .start(&StartDownloadInput::new(ENGINE_REF))
                .await?;
        }
        let model_state = current
            .models
            .iter()
            .find(|candidate| candidate.model_ref == model.item_ref)
            .map_or(CatalogueItemState::Available, |candidate| candidate.state);
        if missing(model_state) {
            downloads
                .start(&StartDownloadInput::new(model.item_ref))
                .await?;
        }
        self.log(
            LogLevel::Info,
            "Queued a speech model install",
            &[("modelRef", json!(model.item_ref))],
        );
        self.models().await
    }

    pub async fn select_model(
        &self,
        input: &VoiceSelectModelInput,
    ) -> Result<VoiceModelsDto, AppError> {
        let value = match &input.model_ref {
            None => Value::Null,
            Some(model_ref) => {
                let model = speech_model(model_ref).ok_or_else(|| {
                    AppError::new(AppErrorCode::NotFound, SPEECH_MODEL_UNKNOWN)
                        .with_detail("modelRef", model_ref.clone())
                })?;
                if !self.model_path(model).is_file() {
                    return Err(
                        AppError::new(AppErrorCode::Conflict, SPEECH_MODEL_NOT_INSTALLED)
                            .with_detail("modelRef", model_ref.clone()),
                    );
                }
                json!(model.item_ref)
            }
        };
        self.options()
            .settings
            .set(VOICE_MODEL_SETTING_KEY, &value)?;
        self.models().await
    }

    fn model_path(&self, model: &SpeechModel) -> PathBuf {
        self.options()
            .downloads_dir
            .join(DownloadKind::Model.folder())
            .join(model.file_name)
    }

    fn engine_executable(&self) -> Option<PathBuf> {
        let stored = self
            .options()
            .settings
            .get(VOICE_ENGINE_SETTING_KEY)
            .ok()
            .flatten()?;
        let path = stored.value.get("executablePath")?.as_str()?;
        let path = PathBuf::from(path);
        path.is_file().then_some(path)
    }

    fn engine_ready(&self) -> bool {
        self.engine_executable().is_some()
    }

    fn ready(&self) -> Result<(PathBuf, PathBuf), AppError> {
        let not_ready = || AppError::new(AppErrorCode::Conflict, SPEECH_NOT_READY);
        let executable = self.engine_executable().ok_or_else(not_ready)?;
        let model = self.selected_model()?.ok_or_else(not_ready)?;
        let path = self.model_path(model);
        if !path.is_file() {
            return Err(not_ready());
        }
        Ok((executable, path))
    }

    async fn recording_device(&self) -> Result<Option<String>, AppError> {
        let Some(stored) = self.stored_device()? else {
            return Ok(None);
        };
        let connected = self
            .capture_devices()
            .await
            .map(|devices| devices.iter().any(|device| device.id == stored))
            .unwrap_or(false);
        Ok(connected.then_some(stored))
    }

    pub async fn start_recording(&self) -> Result<(), AppError> {
        self.ready()?;
        if lock(&self.inner.recording).is_some() {
            return Err(AppError::new(AppErrorCode::Conflict, ALREADY_RECORDING));
        }
        let device = self.recording_device().await?;
        let audio = Arc::clone(&self.options().audio);
        let recording = tokio::task::spawn_blocking(move || audio.record(device.as_deref()))
            .await
            .map_err(|error| native(RECORDING_FAILED, error.to_string()))?
            .map_err(|reason| native(RECORDING_FAILED, reason))?;
        let mut slot = lock(&self.inner.recording);
        if slot.is_some() {
            drop(slot);
            let _ = recording.finish();
            return Err(AppError::new(AppErrorCode::Conflict, ALREADY_RECORDING));
        }
        *slot = Some(recording);
        Ok(())
    }

    fn take_recording(&self) -> Result<Box<dyn Recording>, AppError> {
        lock(&self.inner.recording)
            .take()
            .ok_or_else(|| AppError::new(AppErrorCode::Conflict, NOT_RECORDING))
    }

    pub async fn cancel_recording(&self) -> Result<(), AppError> {
        let recording = self.take_recording()?;
        let _ = tokio::task::spawn_blocking(move || recording.finish()).await;
        Ok(())
    }

    pub async fn stop_recording(&self) -> Result<VoiceTranscriptDto, AppError> {
        let recording = self.take_recording()?;
        let mut samples = tokio::task::spawn_blocking(move || recording.finish())
            .await
            .map_err(|error| native(whisper::RECOGNITION_FAILED, error.to_string()))?
            .map_err(|reason| native(whisper::RECOGNITION_FAILED, reason))?;
        if samples.len() < MIN_SPEECH_SAMPLES {
            return Ok(transcript(String::new()));
        }
        samples.truncate(MAX_RECORDING_SAMPLES);
        let (executable, model) = self.ready()?;
        let directory = self.options().cache_dir.join("voice");
        let id = crate::platform::ids::new_id().to_string();
        let audio = directory.join(format!("{id}.wav"));
        tokio::fs::create_dir_all(&directory)
            .await
            .map_err(|error| native(whisper::RECOGNITION_FAILED, error.to_string()))?;
        tokio::fs::write(&audio, wav_bytes(&samples))
            .await
            .map_err(|error| native(whisper::RECOGNITION_FAILED, error.to_string()))?;
        let request = TranscribeRequest {
            executable,
            model,
            audio: audio.clone(),
            output_base: directory.join(&id),
            threads: std::thread::available_parallelism()
                .map_or(4, std::num::NonZeroUsize::get)
                .min(MAX_THREADS),
        };
        let outcome = self.options().recognizer.transcribe(&request).await;
        let _ = tokio::fs::remove_file(&audio).await;
        let text = outcome?;
        self.log(
            LogLevel::Info,
            "Recognised a dictation",
            &[
                ("seconds", json!(samples.len() / SAMPLE_RATE as usize)),
                ("chars", json!(text.chars().count())),
            ],
        );
        Ok(transcript(text))
    }

    async fn unpack_engine(&self, row: &DownloadRow) -> Result<(), AppError> {
        let downloads_dir = &self.options().downloads_dir;
        let target = runtime_install_path(downloads_dir, ENGINE_RUNTIME_ID)
            .map_err(|error| AppError::new(AppErrorCode::Unknown, error.to_string()))?;
        if target.exists() {
            let removal = target.clone();
            let _ = tokio::task::spawn_blocking(move || std::fs::remove_dir_all(removal)).await;
        }
        let report = extract_archive(
            Path::new(&row.target_path),
            &target,
            ExtractOptions::default(),
        )
        .await?;
        let _ = std::fs::remove_file(&row.target_path);
        let root = target.clone();
        let executable = tokio::task::spawn_blocking(move || whisper::find_cli(&root, 0))
            .await
            .ok()
            .flatten()
            .ok_or_else(|| AppError::new(AppErrorCode::UnsupportedFormat, ENGINE_UNPACK_FAILED))?;
        self.options().settings.set(
            VOICE_ENGINE_SETTING_KEY,
            &json!({
                "version": row.version,
                "installPath": target.to_string_lossy(),
                "executablePath": executable.to_string_lossy(),
                "sizeBytes": report.bytes,
            }),
        )?;
        self.log(
            LogLevel::Info,
            "Unpacked the speech engine",
            &[("files", json!(report.files))],
        );
        Ok(())
    }

    fn adopt_model(&self, model: &SpeechModel) -> Result<(), AppError> {
        if self.selected_model()?.is_none() {
            self.options()
                .settings
                .set(VOICE_MODEL_SETTING_KEY, &json!(model.item_ref))?;
        }
        Ok(())
    }
}

#[async_trait]
impl DownloadInstaller for VoiceService {
    async fn install(&self, row: &DownloadRow) -> Result<(), AppError> {
        if row.item_ref == ENGINE_REF {
            return self.unpack_engine(row).await;
        }
        if let Some(model) = speech_model(&row.item_ref) {
            return self.adopt_model(model);
        }
        Ok(())
    }
}
