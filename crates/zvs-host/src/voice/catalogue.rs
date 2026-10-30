use async_trait::async_trait;
use zvs_contract::{
    AppError, CatalogueSource, ChecksumAlgorithm, ChecksumDto, DeviceProfileDto, DeviceTier,
    DownloadItemKind,
};

use crate::downloads::{CatalogueItem, CatalogueProvider};
use crate::runtimes::Platform;

pub const ENGINE_REF: &str = "curated:runtime:whisper-cpp-blas";
pub const ENGINE_VERSION: &str = "v1.9.2";
pub const ENGINE_FILE: &str = "whisper-blas-bin-x64.zip";
const ENGINE_URL: &str =
    "https://github.com/ggml-org/whisper.cpp/releases/download/v1.9.2/whisper-blas-bin-x64.zip";
const ENGINE_SHA256: &str = "ffe5b47ca8e53a7677949f23a9c4641bbec4eee8a5714c3d14b67bb8d7b24a78";
const ENGINE_BYTES: u64 = 20_981_386;
const MODEL_BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpeechModel {
    pub item_ref: &'static str,
    pub name: &'static str,
    pub display_name: &'static str,
    pub description: &'static str,
    pub version: &'static str,
    pub file_name: &'static str,
    pub size_bytes: u64,
    pub sha256: &'static str,
    pub tier: DeviceTier,
}

pub const SPEECH_MODELS: &[SpeechModel] = &[
    SpeechModel {
        item_ref: "curated:speech:whisper-base-q8_0",
        name: "whisper-base",
        display_name: "Whisper Base",
        description: "Лёгкая модель распознавания речи: быстро работает на любом процессоре, но чаще ошибается в длинных фразах и терминах.",
        version: "Q8_0",
        file_name: "ggml-base-q8_0.bin",
        size_bytes: 81_768_585,
        sha256: "c577b9a86e7e048a0b7eada054f4dd79a56bbfa911fbdacf900ac5b567cbb7d9",
        tier: DeviceTier::Low,
    },
    SpeechModel {
        item_ref: "curated:speech:whisper-small-q8_0",
        name: "whisper-small",
        display_name: "Whisper Small",
        description: "Сбалансированная модель распознавания речи: уверенно распознаёт русский и английский, фраза в несколько секунд обрабатывается за секунду-две на современном процессоре.",
        version: "Q8_0",
        file_name: "ggml-small-q8_0.bin",
        size_bytes: 264_464_607,
        sha256: "49c8fb02b65e6049d5fa6c04f81f53b867b5ec9540406812c643f177317f779f",
        tier: DeviceTier::Medium,
    },
    SpeechModel {
        item_ref: "curated:speech:whisper-large-v3-turbo-q5_0",
        name: "whisper-large-v3-turbo",
        display_name: "Whisper Large v3 Turbo",
        description: "Самая точная модель распознавания речи из набора: лучше справляется с акцентом, шумом и специальной лексикой. Требует мощного процессора и около 2 ГБ оперативной памяти во время работы.",
        version: "Q5_0",
        file_name: "ggml-large-v3-turbo-q5_0.bin",
        size_bytes: 574_041_195,
        sha256: "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
        tier: DeviceTier::High,
    },
];

pub const ENGINE_DISPLAY_NAME: &str = "whisper.cpp · CPU";
const ENGINE_DESCRIPTION: &str = "Движок локального распознавания речи для моделей Whisper. Считает на процессоре с ускорением OpenBLAS и не отправляет звук в сеть.";
const SPEECH_TAG: &str = "распознавание речи";

pub fn speech_supported(platform: Platform) -> bool {
    platform.os == "win32" && platform.arch == "x64"
}

pub fn speech_model(item_ref: &str) -> Option<&'static SpeechModel> {
    SPEECH_MODELS
        .iter()
        .find(|model| model.item_ref == item_ref)
}

fn gib(bytes: u64) -> u64 {
    (bytes as f64 / f64::from(1_u32 << 30)).round() as u64
}

pub fn recommend(device: Option<&DeviceProfileDto>) -> (&'static SpeechModel, String) {
    let Some(device) = device else {
        return (
            &SPEECH_MODELS[1],
            "Сведения о компьютере недоступны, поэтому предлагается сбалансированная модель."
                .to_owned(),
        );
    };
    let model = SPEECH_MODELS
        .iter()
        .find(|model| model.tier == device.tier)
        .unwrap_or(&SPEECH_MODELS[1]);
    let summary = format!(
        "{} ГБ оперативной памяти, ядер процессора: {}",
        gib(device.total_memory_bytes),
        device.cpu_cores
    );
    let reason = match device.tier {
        DeviceTier::Low => {
            format!("{summary}. Лёгкая модель не будет замедлять работу компьютера.")
        }
        DeviceTier::Medium => format!("{summary}. Этого достаточно для сбалансированной модели."),
        DeviceTier::High => {
            format!("{summary}. Ресурсов хватает для самой точной модели.")
        }
    };
    (model, reason)
}

fn checksum(value: &str) -> Option<ChecksumDto> {
    Some(ChecksumDto {
        algorithm: ChecksumAlgorithm::Sha256,
        value: value.to_owned(),
    })
}

pub fn engine_item() -> CatalogueItem {
    CatalogueItem {
        display_name: ENGINE_DISPLAY_NAME.to_owned(),
        description: ENGINE_DESCRIPTION.to_owned(),
        version: Some(ENGINE_VERSION.to_owned()),
        size_bytes: ENGINE_BYTES,
        url: ENGINE_URL.to_owned(),
        file_name: ENGINE_FILE.to_owned(),
        checksum: checksum(ENGINE_SHA256),
        tags: vec![
            "whisper.cpp".to_owned(),
            "cpu".to_owned(),
            SPEECH_TAG.to_owned(),
        ],
        ..CatalogueItem::new(
            ENGINE_REF,
            DownloadItemKind::Runtime,
            CatalogueSource::Curated,
            "whisper-cpp-blas",
        )
    }
}

pub fn model_item(model: &SpeechModel) -> CatalogueItem {
    CatalogueItem {
        display_name: model.display_name.to_owned(),
        description: model.description.to_owned(),
        version: Some(model.version.to_owned()),
        size_bytes: model.size_bytes,
        url: format!("{MODEL_BASE_URL}/{}", model.file_name),
        file_name: model.file_name.to_owned(),
        checksum: checksum(model.sha256),
        tags: vec![
            "Whisper".to_owned(),
            SPEECH_TAG.to_owned(),
            model.version.to_owned(),
        ],
        ..CatalogueItem::new(
            model.item_ref,
            DownloadItemKind::Model,
            CatalogueSource::Curated,
            model.name,
        )
    }
}

pub struct SpeechCatalogueProvider {
    pub platform: Platform,
}

#[async_trait]
impl CatalogueProvider for SpeechCatalogueProvider {
    fn id(&self) -> CatalogueSource {
        CatalogueSource::Curated
    }

    fn live(&self) -> bool {
        false
    }

    async fn list(&self) -> Result<Vec<CatalogueItem>, AppError> {
        if !speech_supported(self.platform) {
            return Ok(Vec::new());
        }
        Ok(std::iter::once(engine_item())
            .chain(SPEECH_MODELS.iter().map(model_item))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use zvs_contract::Validate;

    use super::*;
    use crate::platform::paths::download_target_path;

    fn device(tier: DeviceTier) -> DeviceProfileDto {
        DeviceProfileDto {
            platform: "win32".to_owned(),
            arch: "x64".to_owned(),
            cpu_model: "Test CPU".to_owned(),
            cpu_cores: 8,
            total_memory_bytes: 16 << 30,
            free_memory_bytes: 8 << 30,
            free_disk_bytes: 100 << 30,
            gpu: None,
            tier,
            measured_at: 1,
        }
    }

    #[test]
    fn the_recommendation_follows_the_device_tier_and_says_why() {
        let (low, _) = recommend(Some(&device(DeviceTier::Low)));
        assert_eq!(low.name, "whisper-base");
        let (medium, reason) = recommend(Some(&device(DeviceTier::Medium)));
        assert_eq!(medium.name, "whisper-small");
        assert!(reason.contains("16 ГБ"));
        assert!(reason.contains('8'));
        let (high, _) = recommend(Some(&device(DeviceTier::High)));
        assert_eq!(high.name, "whisper-large-v3-turbo");
        assert_eq!(recommend(None).0.name, "whisper-small");
    }

    #[test]
    fn every_offered_item_is_downloadable_and_fits_the_contract() {
        for item in std::iter::once(engine_item()).chain(SPEECH_MODELS.iter().map(model_item)) {
            assert!(item.checksum.validate().is_ok(), "{}", item.item_ref);
            assert!(
                download_target_path(
                    std::path::Path::new("downloads"),
                    item.kind.as_str(),
                    &item.file_name
                )
                .is_ok(),
                "{}",
                item.file_name
            );
        }
        assert!(speech_supported(Platform::new("win32", "x64")));
        assert!(!speech_supported(Platform::new("linux", "x64")));
    }
}
