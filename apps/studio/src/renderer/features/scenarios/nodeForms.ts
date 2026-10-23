import type { Json, ScenarioNodeTypeDto, ScenarioParams } from "@zvs/shared";
import { entityKindOf } from "./entityParams";

export type FieldKind = "text" | "multiline" | "number";

export interface ParamField {
  readonly key: string;
  readonly label: string;
  readonly kind?: FieldKind;
  readonly hint?: string;
  readonly placeholder?: string;
}

export interface NodeForm {
  readonly description: string;
  readonly fields: readonly ParamField[];
}

export interface FormField extends ParamField {
  readonly required: boolean;
  readonly entity: boolean;
}

export const OUTPUT_PREFIX = "output.";

const TEMPLATE_HINT =
  "Пусто — отправляется результат предыдущего шага целиком. {{result}} подставляет результат в текст.";

const TEMPLATE: ParamField = {
  key: "template",
  label: "Текст",
  kind: "multiline",
  hint: TEMPLATE_HINT,
  placeholder: "{{result}}",
};

const MODEL_FIELDS: readonly ParamField[] = [
  { key: "providerId", label: "Провайдер" },
  { key: "modelId", label: "Модель" },
];

export const NODE_FORMS: Readonly<Record<string, NodeForm>> = {
  "trigger.chat": {
    description:
      "Запускает сценарий, когда в диалог приходит сообщение. Без выбранного диалога срабатывает на сообщения во всех диалогах.",
    fields: [{ key: "conversationId", label: "Диалог" }],
  },
  "trigger.mail": {
    description: "Запускает сценарий при получении письма, подходящего под условия.",
    fields: [
      { key: "from", label: "Отправитель", placeholder: "support@example.com" },
      { key: "subject", label: "Тема содержит" },
    ],
  },
  "trigger.schedule": {
    description: "Запускает сценарий по расписанию.",
    fields: [
      {
        key: "cron",
        label: "Расписание (cron)",
        placeholder: "0 9 * * 1-5",
        hint: "Минуты, часы, день месяца, месяц, день недели. Пример — по будням в 9:00.",
      },
    ],
  },
  "trigger.webhook": {
    description: "Запускает сценарий HTTP-запросом POST на указанный путь.",
    fields: [{ key: "path", label: "Путь", placeholder: "support/incoming" }],
  },
  "agent.tools": {
    description:
      "Обрабатывает входные данные чат-моделью и вызывает инструменты из подключённых наборов. Чат-модель обязательна, память — по желанию.",
    fields: [
      { key: "systemMessage", label: "Системное сообщение", kind: "multiline" },
      { key: "maxIterations", label: "Максимум шагов", kind: "number", placeholder: "8" },
    ],
  },
  "model.chat": {
    description: "Чат-модель, которой пользуется ИИ-агент. Подключается к слоту «Чат-модель».",
    fields: [
      ...MODEL_FIELDS,
      { key: "temperature", label: "Температура", kind: "number", placeholder: "0.7" },
    ],
  },
  "memory.window": {
    description:
      "Хранит последние сообщения, чтобы агент учитывал контекст. Подключается к слоту «Память».",
    fields: [{ key: "size", label: "Сообщений в памяти", kind: "number", placeholder: "10" }],
  },
  "embedding.model": {
    description: "Модель, которая превращает текст в векторы для поиска по хранилищу.",
    fields: MODEL_FIELDS,
  },
  "tool.set": {
    description:
      "Набор инструментов, которые ИИ-агент может вызывать. Подключается к слоту «Инструменты».",
    fields: [],
  },
  "output.chatReply": {
    description:
      "Отправляет результат ответом в диалог, из которого пришло сообщение. Используется с триггером «Сообщение в чате».",
    fields: [TEMPLATE],
  },
  "output.chatMessage": {
    description: "Добавляет результат сообщением в выбранный диалог.",
    fields: [{ key: "conversationId", label: "Диалог" }, TEMPLATE],
  },
  "output.newChat": {
    description: "Создаёт новый диалог, первым сообщением которого становится результат.",
    fields: [
      { key: "title", label: "Название диалога", placeholder: "Отчёт за {{date}}" },
      TEMPLATE,
    ],
  },
  "output.notify": {
    description: "Показывает системное уведомление с результатом.",
    fields: [{ key: "title", label: "Заголовок" }, TEMPLATE],
  },
  "output.file": {
    description: "Сохраняет результат в файл.",
    fields: [{ key: "path", label: "Путь к файлу", placeholder: "reports/support.md" }, TEMPLATE],
  },
  "flow.branch": {
    description: "Направляет выполнение по одной из ветвей в зависимости от условия.",
    fields: [],
  },
  "flow.map": {
    description: "Выполняет следующие шаги для каждого элемента списка.",
    fields: [],
  },
  "job.run": { description: "Запускает фоновую задачу.", fields: [] },
};

export function isOutputType(type: string): boolean {
  return type.startsWith(OUTPUT_PREFIX);
}

export function descriptionOf(type: string): string | null {
  return NODE_FORMS[type]?.description ?? null;
}

export function formFields(spec: ScenarioNodeTypeDto | undefined, type: string): FormField[] {
  const required = new Set(spec?.requiredParams ?? []);
  const declared = NODE_FORMS[type]?.fields ?? [];
  const fields: FormField[] = declared.map((field) => ({
    ...field,
    required: required.has(field.key),
    entity: entityKindOf(field.key) !== null,
  }));
  for (const key of required) {
    if (declared.some((field) => field.key === key)) continue;
    fields.push({ key, label: key, required: true, entity: entityKindOf(key) !== null });
  }
  return fields;
}

export function fieldValue(field: ParamField, text: string): Json | undefined {
  const trimmed = text.trim();
  if (trimmed.length === 0) return undefined;
  if (field.kind !== "number") return text;
  const number = Number(trimmed.replace(",", "."));
  return Number.isFinite(number) ? number : text;
}

export function withField(params: ScenarioParams, field: ParamField, text: string): ScenarioParams {
  const next: ScenarioParams = { ...params };
  const value = fieldValue(field, text);
  if (value === undefined) delete next[field.key];
  else next[field.key] = value;
  return next;
}
