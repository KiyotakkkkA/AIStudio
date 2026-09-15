import type { CatalogueItem } from "./catalogue.ts";

export const CURATED_CATALOGUE: readonly CatalogueItem[] = [
  {
    ref: "curated:model:qwen2.5-vl-3b-instruct-q4_k_m",
    kind: "model",
    source: "curated",
    name: "qwen2.5-vl:3b",
    displayName: "Qwen2.5-VL 3B Instruct (OCR)",
    description:
      "Зрительно-языковая модель для распознавания текста: сканы, фотографии документов, таблицы и рукописный текст, включая русский. Вместе с проектором занимает около 2,8 ГБ и помещается на видеокарту 8 ГБ рядом с моделью эмбеддингов. Вариант по умолчанию для машин с 16 ГБ ОЗУ.",
    version: "Q4_K_M",
    sizeBytes: 1_929_901_056,
    fileName: "Qwen2.5-VL-3B-Instruct-Q4_K_M.gguf",
    url: "https://huggingface.co/ggml-org/Qwen2.5-VL-3B-Instruct-GGUF/resolve/main/Qwen2.5-VL-3B-Instruct-Q4_K_M.gguf",
    checksum: {
      algorithm: "sha256",
      value: "d02fe9b69ad8cadbbd228e387667af66612c44bed29ffc8eb1e7caf9ac486c12",
    },
    tags: ["Qwen", "OCR", "3B", "зрение", "Q4_K_M", "8 ГБ VRAM"],
  },
  {
    ref: "curated:model:qwen2.5-vl-3b-instruct-mmproj-q8_0",
    kind: "model",
    source: "curated",
    name: "qwen2.5-vl:3b-mmproj",
    displayName: "Qwen2.5-VL 3B — проектор mmproj (Для QWEN2.5-VL 3B)",
    description:
      "Зрительная часть Qwen2.5-VL 3B: без неё модель не видит изображение и распознавание не запустится. Скачивается в пару к самой модели и лежит рядом с ней.",
    version: "Q8_0",
    sizeBytes: 844_757_728,
    fileName: "mmproj-Qwen2.5-VL-3B-Instruct-Q8_0.gguf",
    url: "https://huggingface.co/ggml-org/Qwen2.5-VL-3B-Instruct-GGUF/resolve/main/mmproj-Qwen2.5-VL-3B-Instruct-Q8_0.gguf",
    checksum: {
      algorithm: "sha256",
      value: "980c9b2f78c04e6cff93d277ada09e768394f112d75db3b4e9dea8a69f9fb904",
    },
    tags: ["Qwen", "OCR", "3B", "проектор", "Q8_0"],
  },
  {
    ref: "curated:model:qwen2.5-vl-7b-instruct-q4_k_m",
    kind: "model",
    source: "curated",
    name: "qwen2.5-vl:7b",
    displayName: "Qwen2.5-VL 7B Instruct (OCR)",
    description:
      "Та же модель крупнее: заметно точнее на плохих сканах и рукописном тексте. Вместе с проектором занимает около 5,5 ГБ и требует видеокарту от 12 ГБ, если рядом должна работать модель эмбеддингов. На 8 ГБ берите версию 3B.",
    version: "Q4_K_M",
    sizeBytes: 4_683_072_032,
    fileName: "Qwen2.5-VL-7B-Instruct-Q4_K_M.gguf",
    url: "https://huggingface.co/ggml-org/Qwen2.5-VL-7B-Instruct-GGUF/resolve/main/Qwen2.5-VL-7B-Instruct-Q4_K_M.gguf",
    checksum: {
      algorithm: "sha256",
      value: "9258bf05b12686d097ff3b6b18d968ab393649780aa2b3cd67fec43d50554392",
    },
    tags: ["Qwen", "OCR", "7B", "зрение", "Q4_K_M", "12 ГБ VRAM"],
  },
  {
    ref: "curated:model:qwen2.5-vl-7b-instruct-mmproj-q8_0",
    kind: "model",
    source: "curated",
    name: "qwen2.5-vl:7b-mmproj",
    displayName: "Qwen2.5-VL 7B — проектор mmproj (Для QWEN2.5-VL 7B)",
    description:
      "Зрительная часть Qwen2.5-VL 7B: без неё модель не видит изображение и распознавание не запустится. Скачивается в пару к самой модели и лежит рядом с ней.",
    version: "Q8_0",
    sizeBytes: 853_119_712,
    fileName: "mmproj-Qwen2.5-VL-7B-Instruct-Q8_0.gguf",
    url: "https://huggingface.co/ggml-org/Qwen2.5-VL-7B-Instruct-GGUF/resolve/main/mmproj-Qwen2.5-VL-7B-Instruct-Q8_0.gguf",
    checksum: {
      algorithm: "sha256",
      value: "2ddb555391bae966e412deab9e07b58afa18bcc06930ba0f1c78a3695ab9e506",
    },
    tags: ["Qwen", "OCR", "7B", "проектор", "Q8_0"],
  },
  {
    ref: "curated:embedding:bge-m3-f16",
    kind: "embedding",
    source: "curated",
    name: "bge-m3",
    displayName: "BGE-M3",
    description:
      "Многоязычная модель эмбеддингов: 100+ языков, контекст до 8192 токенов, одинаково хорошо работает на русском и английском. База для векторных хранилищ.",
    version: "F16",
    sizeBytes: 1_157_671_200,
    dimension: 1024,
    fileName: "bge-m3-FP16.gguf",
    url: "https://huggingface.co/gpustack/bge-m3-GGUF/resolve/main/bge-m3-FP16.gguf",
    checksum: {
      algorithm: "sha256",
      value: "daec91ffb5dd0c27411bd71f29932917c49cf529a641d0168496c3a501e3062c",
    },
    tags: ["BAAI", "1024 dim", "8k ctx", "многоязычная"],
  },
  {
    ref: "curated:model:bge-reranker-v2-m3-f16",
    kind: "model",
    source: "curated",
    name: "bge-reranker-v2-m3",
    displayName: "BGE Reranker v2 M3",
    description:
      "Кросс-энкодер для переранжирования: получает запрос и найденные фрагменты и заново их упорядочивает. Ставится рядом с BGE-M3 и заметно поднимает точность выдачи.",
    version: "F16",
    sizeBytes: 1_159_776_896,
    fileName: "bge-reranker-v2-m3-FP16.gguf",
    url: "https://huggingface.co/gpustack/bge-reranker-v2-m3-GGUF/resolve/main/bge-reranker-v2-m3-FP16.gguf",
    checksum: {
      algorithm: "sha256",
      value: "5df93be121c09c43432102ad2b9569d369ccb85c209ca7583e8ccd28f0e41b88",
    },
    tags: ["BAAI", "переранжирование", "многоязычная"],
  },
];
