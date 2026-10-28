import type { Json, ScenarioNodeTypeDto, ScenarioParams } from "@zvs/shared";
import { entityKindOf } from "./entityParams";
import { STORES_PARAM, STORES_TYPE } from "./nodeRules";

export type FieldKind = "text" | "multiline" | "number" | "boolean" | "json" | "select";

export interface ParamField {
  readonly key: string;
  readonly label: string;
  readonly kind?: FieldKind;
  readonly hint?: string;
  readonly placeholder?: string;
  readonly options?: readonly string[];
  readonly optionLabels?: Readonly<Record<string, string>>;
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
  "trigger.integration": {
    description:
      "Запускает сценарий, когда подключённая интеграция сообщает о событии. Повтор события с тем же идентификатором сценарий не запускает.",
    fields: [
      { key: "accountId", label: "Интеграция" },
      {
        key: "trigger",
        label: "Событие",
        placeholder: "telegram.message",
        hint: "Ключ события из панели автоматизации интеграции.",
      },
    ],
  },
  "agent.tools": {
    description:
      "Обрабатывает входные данные чат-моделью и вызывает инструменты из подключённых наборов. Чат-модель обязательна; история диалога, память и векторные хранилища подключаются к слоту «Память» по необходимости.",
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
      "Передаёт агенту последние сообщения диалога как контекст. Подключается к слоту «Память».",
    fields: [{ key: "size", label: "Сообщений в истории", kind: "number", placeholder: "10" }],
  },
  "memory.facts": {
    description:
      "Добавляет агенту сохранённые факты памяти. В режиме «Чтение и запись» агент получает инструмент памяти. Подключается к слоту «Память».",
    fields: [
      {
        key: "policy",
        label: "Доступ",
        kind: "select",
        options: ["read", "readWrite"],
        optionLabels: { read: "Только чтение", readWrite: "Чтение и запись" },
      },
      {
        key: "area",
        label: "Область памяти",
        kind: "select",
        options: ["global", "dialog", "both"],
        optionLabels: { global: "Глобальная", dialog: "Диалог", both: "Глобальная и диалог" },
        hint: "Память диалога требует триггера «Сообщение в чате».",
      },
    ],
  },
  "knowledge.stores": {
    description:
      "Даёт агенту поиск по выбранным векторным хранилищам. Подключается к слоту «Память».",
    fields: [],
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
    description:
      "Направляет выполнение по выходу «да» или «нет» в зависимости от условия. Шаги другой ветви пропускаются.",
    fields: [
      {
        key: "condition",
        label: "Условие",
        placeholder: "{{result.ok}}",
        hint: "Ссылка на логическое поле результата предыдущего шага или значение true / false.",
      },
    ],
  },
  "flow.map": {
    description: "Выполняет следующие шаги для каждого элемента списка.",
    fields: [],
  },
};

export function isOutputType(type: string): boolean {
  return type.startsWith(OUTPUT_PREFIX);
}

export function descriptionOf(type: string): string | null {
  return NODE_FORMS[type]?.description ?? null;
}

function record(value: unknown): Readonly<Record<string, unknown>> | null {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function schemaType(schema: Readonly<Record<string, unknown>>): string | null {
  const declared = schema.type;
  if (typeof declared === "string") return declared;
  if (Array.isArray(declared)) {
    const concrete = declared.find((entry) => entry !== "null");
    return typeof concrete === "string" ? concrete : null;
  }
  return null;
}

function schemaKind(schema: Readonly<Record<string, unknown>>): FieldKind {
  const options = schema.enum;
  if (Array.isArray(options) && options.every((option) => typeof option === "string")) {
    return "select";
  }
  const kind = schemaType(schema);
  if (kind === "integer" || kind === "number") return "number";
  if (kind === "boolean") return "boolean";
  if (kind === "string") return "text";
  return "json";
}

export function schemaFields(schema: Json | undefined): ParamField[] {
  const root = record(schema);
  const properties = record(root?.properties);
  if (properties === null) return [];
  return Object.entries(properties).flatMap(([key, value]) => {
    const property = record(value);
    if (property === null) return [];
    const kind = schemaKind(property);
    const title = typeof property.title === "string" ? property.title : key;
    const description = typeof property.description === "string" ? property.description : undefined;
    const options = kind === "select" ? (property.enum as string[]) : undefined;
    return [
      {
        key,
        label: title,
        kind,
        ...(description === undefined ? {} : { hint: description }),
        ...(options === undefined ? {} : { options }),
      },
    ];
  });
}

export function formFields(spec: ScenarioNodeTypeDto | undefined, type: string): FormField[] {
  const required = new Set(spec?.requiredParams ?? []);
  const declared = NODE_FORMS[type]?.fields ?? [];
  const hidden = new Set(type === STORES_TYPE ? [STORES_PARAM] : []);
  const fields: FormField[] = declared.map((field) => ({
    ...field,
    required: required.has(field.key),
    entity: entityKindOf(field.key) !== null,
  }));
  const known = new Set([...declared.map((field) => field.key), ...hidden]);
  for (const field of schemaFields(spec?.paramsSchema)) {
    if (known.has(field.key)) continue;
    known.add(field.key);
    fields.push({
      ...field,
      required: required.has(field.key),
      entity: entityKindOf(field.key) !== null,
    });
  }
  for (const key of required) {
    if (known.has(key)) continue;
    fields.push({ key, label: key, required: true, entity: entityKindOf(key) !== null });
  }
  return fields;
}

export function optionLabel(field: ParamField, option: string): string {
  return field.optionLabels?.[option] ?? option;
}

export function fieldValue(field: ParamField, text: string): Json | undefined {
  const trimmed = text.trim();
  if (trimmed.length === 0) return undefined;
  if (field.kind === "json") {
    try {
      return JSON.parse(trimmed) as Json;
    } catch {
      return text;
    }
  }
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
