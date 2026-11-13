export const ASSISTANT_SECTION_TITLE = "Взаимодействие с ассистентом";

export const SUMMARIZER_ROW = {
  key: "assistant.summarizer",
  label: "Модель для сводок",
  hint: "Формирование сводок диалогов и кратких итогов выполнения запросов.",
} as const;

export const SUMMARIZER_MODELS_ROW = {
  key: "assistant.summarizerModels",
  label: "Предпочтительные модели",
  hint: "Используется первая доступная модель из списка. Если доступных моделей нет, используется модель диалога.",
} as const;

export const AUTO_COMPACT_ROW = {
  key: "assistant.autoCompact",
  label: "Автоматическое сжатие",
  hint: "Сжатие диалога при заполнении более 80 % доступного контекста.",
} as const;

export const RECAPS_ROW = {
  key: "assistant.recaps",
  label: "Краткие итоги",
  hint: "Краткие итоги запросов с вызовами инструментов или в режиме цели.",
} as const;

export const SUMMARIZER_MODELS_EMPTY = "Модели для сводок не заданы. Используется модель диалога.";
export const SUMMARIZER_PROVIDER_LABEL = "Провайдер модели для сводок";
export const SUMMARIZER_MODEL_LABEL = "Модель для сводок";
export const ASSISTANT_SAVE_FAILED = "Не удалось сохранить настройки ассистента";
