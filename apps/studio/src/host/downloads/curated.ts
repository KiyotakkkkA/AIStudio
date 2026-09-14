import type { CatalogueItem } from "./catalogue.ts";

export const CURATED_CATALOGUE: readonly CatalogueItem[] = [
  {
    ref: "curated:model:qwen2.5-vl-7b-instruct-q4_k_m",
    kind: "model",
    source: "curated",
    name: "qwen2.5-vl:7b",
    displayName: "Qwen2.5-VL 7B Instruct (OCR)",
    description:
      "Зрительно-языковая модель для локального распознавания текста: сканы, фотографии документов, таблицы и рукописный текст, включая русский. Запускается на видеокарте от 8 ГБ. Работает в паре с проектором mmproj из того же репозитория.",
    version: "Q4_K_M",
    sizeBytes: 4_683_072_032,
    fileName: "Qwen2.5-VL-7B-Instruct-Q4_K_M.gguf",
    url: "https://huggingface.co/ggml-org/Qwen2.5-VL-7B-Instruct-GGUF/resolve/main/Qwen2.5-VL-7B-Instruct-Q4_K_M.gguf",
    checksum: {
      algorithm: "sha256",
      value: "9258bf05b12686d097ff3b6b18d968ab393649780aa2b3cd67fec43d50554392",
    },
    tags: ["Qwen", "OCR", "зрение", "Q4_K_M", "32k ctx"],
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
