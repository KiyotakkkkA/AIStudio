use zvs_contract::{ChecksumAlgorithm, ChecksumDto, DownloadItemKind};

use super::catalogue::CatalogueItem;

struct Curated {
    item_ref: &'static str,
    kind: DownloadItemKind,
    name: &'static str,
    display_name: &'static str,
    description: &'static str,
    version: &'static str,
    size_bytes: u64,
    dimension: Option<u64>,
    file_name: &'static str,
    url: &'static str,
    sha256: &'static str,
    tags: &'static [&'static str],
}

const CURATED: &[Curated] = &[
    Curated {
        item_ref: "curated:model:qwen2.5-vl-3b-instruct-q4_k_m",
        kind: DownloadItemKind::Model,
        name: "qwen2.5-vl:3b",
        display_name: "Qwen2.5-VL 3B Instruct (OCR)",
        description: "Зрительно-языковая модель для распознавания текста: сканы, фотографии документов, таблицы и рукописный текст, включая русский. Вместе с проектором занимает около 2,8 ГБ и помещается на видеокарту 8 ГБ рядом с моделью эмбеддингов. Вариант по умолчанию для машин с 16 ГБ ОЗУ.",
        version: "Q4_K_M",
        size_bytes: 1_929_901_056,
        dimension: None,
        file_name: "Qwen2.5-VL-3B-Instruct-Q4_K_M.gguf",
        url: "https://huggingface.co/ggml-org/Qwen2.5-VL-3B-Instruct-GGUF/resolve/main/Qwen2.5-VL-3B-Instruct-Q4_K_M.gguf",
        sha256: "d02fe9b69ad8cadbbd228e387667af66612c44bed29ffc8eb1e7caf9ac486c12",
        tags: &["Qwen", "OCR", "3B", "зрение", "Q4_K_M", "8 ГБ VRAM"],
    },
    Curated {
        item_ref: "curated:model:qwen2.5-vl-3b-instruct-mmproj-q8_0",
        kind: DownloadItemKind::Model,
        name: "qwen2.5-vl:3b-mmproj",
        display_name: "Qwen2.5-VL 3B — проектор mmproj (Для QWEN2.5-VL 3B)",
        description: "Зрительная часть Qwen2.5-VL 3B: без неё модель не видит изображение и распознавание не запустится. Скачивается в пару к самой модели и лежит рядом с ней.",
        version: "Q8_0",
        size_bytes: 844_757_728,
        dimension: None,
        file_name: "mmproj-Qwen2.5-VL-3B-Instruct-Q8_0.gguf",
        url: "https://huggingface.co/ggml-org/Qwen2.5-VL-3B-Instruct-GGUF/resolve/main/mmproj-Qwen2.5-VL-3B-Instruct-Q8_0.gguf",
        sha256: "980c9b2f78c04e6cff93d277ada09e768394f112d75db3b4e9dea8a69f9fb904",
        tags: &["Qwen", "OCR", "3B", "проектор", "Q8_0"],
    },
    Curated {
        item_ref: "curated:model:qwen2.5-vl-7b-instruct-q4_k_m",
        kind: DownloadItemKind::Model,
        name: "qwen2.5-vl:7b",
        display_name: "Qwen2.5-VL 7B Instruct (OCR)",
        description: "Та же модель крупнее: заметно точнее на плохих сканах и рукописном тексте. Вместе с проектором занимает около 5,5 ГБ и требует видеокарту от 12 ГБ, если рядом должна работать модель эмбеддингов. На 8 ГБ берите версию 3B.",
        version: "Q4_K_M",
        size_bytes: 4_683_072_032,
        dimension: None,
        file_name: "Qwen2.5-VL-7B-Instruct-Q4_K_M.gguf",
        url: "https://huggingface.co/ggml-org/Qwen2.5-VL-7B-Instruct-GGUF/resolve/main/Qwen2.5-VL-7B-Instruct-Q4_K_M.gguf",
        sha256: "9258bf05b12686d097ff3b6b18d968ab393649780aa2b3cd67fec43d50554392",
        tags: &["Qwen", "OCR", "7B", "зрение", "Q4_K_M", "12 ГБ VRAM"],
    },
    Curated {
        item_ref: "curated:model:qwen2.5-vl-7b-instruct-mmproj-q8_0",
        kind: DownloadItemKind::Model,
        name: "qwen2.5-vl:7b-mmproj",
        display_name: "Qwen2.5-VL 7B — проектор mmproj (Для QWEN2.5-VL 7B)",
        description: "Зрительная часть Qwen2.5-VL 7B: без неё модель не видит изображение и распознавание не запустится. Скачивается в пару к самой модели и лежит рядом с ней.",
        version: "Q8_0",
        size_bytes: 853_119_712,
        dimension: None,
        file_name: "mmproj-Qwen2.5-VL-7B-Instruct-Q8_0.gguf",
        url: "https://huggingface.co/ggml-org/Qwen2.5-VL-7B-Instruct-GGUF/resolve/main/mmproj-Qwen2.5-VL-7B-Instruct-Q8_0.gguf",
        sha256: "2ddb555391bae966e412deab9e07b58afa18bcc06930ba0f1c78a3695ab9e506",
        tags: &["Qwen", "OCR", "7B", "проектор", "Q8_0"],
    },
    Curated {
        item_ref: "curated:embedding:bge-m3-f16",
        kind: DownloadItemKind::Embedding,
        name: "bge-m3",
        display_name: "BGE-M3",
        description: "Многоязычная модель эмбеддингов: 100+ языков, контекст до 8192 токенов, одинаково хорошо работает на русском и английском. База для векторных хранилищ.",
        version: "F16",
        size_bytes: 1_157_671_200,
        dimension: Some(1024),
        file_name: "bge-m3-FP16.gguf",
        url: "https://huggingface.co/gpustack/bge-m3-GGUF/resolve/main/bge-m3-FP16.gguf",
        sha256: "daec91ffb5dd0c27411bd71f29932917c49cf529a641d0168496c3a501e3062c",
        tags: &["BAAI", "1024 dim", "8k ctx", "многоязычная"],
    },
    Curated {
        item_ref: "curated:model:bge-reranker-v2-m3-f16",
        kind: DownloadItemKind::Model,
        name: "bge-reranker-v2-m3",
        display_name: "BGE Reranker v2 M3",
        description: "Кросс-энкодер для переранжирования: получает запрос и найденные фрагменты и заново их упорядочивает. Ставится рядом с BGE-M3 и заметно поднимает точность выдачи.",
        version: "F16",
        size_bytes: 1_159_776_896,
        dimension: None,
        file_name: "bge-reranker-v2-m3-FP16.gguf",
        url: "https://huggingface.co/gpustack/bge-reranker-v2-m3-GGUF/resolve/main/bge-reranker-v2-m3-FP16.gguf",
        sha256: "5df93be121c09c43432102ad2b9569d369ccb85c209ca7583e8ccd28f0e41b88",
        tags: &["BAAI", "переранжирование", "многоязычная"],
    },
];

pub fn curated_catalogue() -> Vec<CatalogueItem> {
    CURATED
        .iter()
        .map(|entry| CatalogueItem {
            display_name: entry.display_name.to_owned(),
            description: entry.description.to_owned(),
            version: Some(entry.version.to_owned()),
            size_bytes: entry.size_bytes,
            url: entry.url.to_owned(),
            file_name: entry.file_name.to_owned(),
            checksum: Some(ChecksumDto {
                algorithm: ChecksumAlgorithm::Sha256,
                value: entry.sha256.to_owned(),
            }),
            tags: entry.tags.iter().map(|tag| (*tag).to_owned()).collect(),
            dimension: entry.dimension,
            ..CatalogueItem::new(
                entry.item_ref,
                entry.kind,
                zvs_contract::CatalogueSource::Curated,
                entry.name,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use zvs_contract::Validate;

    use super::*;
    use crate::platform::paths::download_target_path;

    #[test]
    fn every_curated_entry_is_a_valid_downloadable_row() {
        let items = curated_catalogue();
        assert_eq!(items.len(), 6);
        assert_eq!(
            items
                .iter()
                .find(|item| item.item_ref == "curated:embedding:bge-m3-f16")
                .and_then(|item| item.dimension),
            Some(1024)
        );
        for item in items {
            assert!(
                item.checksum
                    .as_ref()
                    .is_some_and(|sum| sum.validate().is_ok())
            );
            assert!(
                download_target_path(
                    std::path::Path::new("d"),
                    item.kind.as_str(),
                    &item.file_name
                )
                .is_ok()
            );
            assert!(item.url.starts_with("https://huggingface.co/"));
        }
    }
}
