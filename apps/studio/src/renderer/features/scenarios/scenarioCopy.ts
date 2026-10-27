import { isIpcError } from "@zvs/ipc";
import {
  AppErrorCode,
  SCENARIO_FALSE_PORT,
  SCENARIO_TRUE_PORT,
  ScenarioIssueDto,
  type ScenarioIssueRule,
  type ScenarioNodeRole,
  type ScenarioSlot,
} from "@zvs/shared";
import { plural } from "../connections/connectionPresentation";
import type { AttachRefusal, ConnectRefusal, Refusal } from "./graphModel";

export const NODE_TYPE_LABELS: Readonly<Record<string, string>> = {
  "trigger.chat": "Сообщение в чате",
  "trigger.mail": "Новое письмо",
  "trigger.schedule": "Расписание",
  "trigger.webhook": "Вебхук",
  "agent.tools": "ИИ-агент",
  "model.chat": "Чат-модель",
  "memory.window": "История диалога",
  "memory.facts": "Память",
  "knowledge.stores": "Векторные хранилища",
  "embedding.model": "Модель эмбеддингов",
  "tool.set": "Набор инструментов",
  "output.chatReply": "Ответ в чат",
  "output.chatMessage": "Сообщение в диалог",
  "output.newChat": "Новый диалог",
  "output.notify": "Уведомление",
  "output.file": "Сохранение в файл",
  "flow.branch": "Ветвление",
  "flow.map": "Обработка списка",
};

export const ROLE_LABELS: Record<ScenarioNodeRole, string> = {
  trigger: "Триггер",
  action: "Действие",
  subNode: "Подузел",
};

export const SLOT_LABELS: Record<ScenarioSlot, string> = {
  model: "Чат-модель",
  memory: "Память",
  tool: "Инструменты",
  embedding: "Эмбеддинги",
};

export const PORT_LABELS: Readonly<Record<string, string>> = {
  [SCENARIO_TRUE_PORT]: "да",
  [SCENARIO_FALSE_PORT]: "нет",
};

export const PALETTE_GROUP_LABELS = {
  triggers: "Триггеры",
  agents: "Агенты и подузлы",
  memory: "Память",
  outputs: "Результат",
  core: "Действия ядра",
} as const;

export const TOOL_OFF = "Инструмент выключен: разрешение «off»";
export const TOOL_UNAVAILABLE = "Инструмент недоступен";
export const TOOL_UNREGISTERED = "Инструмент не зарегистрирован";
export const TOOL_RESTRICTED = "Недоступен в наборе инструментов";
export const TOOL_SET_EMPTY = "Инструменты не выбраны";
export const STORES_EMPTY = "Хранилища не выбраны";
export const NEXT_STEP = "Добавить следующий узел";

export function nextForSlot(name: string, slot: ScenarioSlot): string {
  return `Подузлы для слота «${SLOT_LABELS[slot]}» узла «${name}»`;
}

export function nextForPort(name: string, port: string): string {
  const label = PORT_LABELS[port];
  return label === undefined
    ? `Шаги после узла «${name}»`
    : `Шаги для выхода «${label}» узла «${name}»`;
}
export const DIALOG_LABEL = "Диалог";
export const DIALOG_FROM_TRIGGER = "Диалог, из которого пришло сообщение";
export const STORES_NONE = "Векторных хранилищ нет";
export const DIALOG_MISSING = "Не определён: нет триггера «Сообщение в чате»";
export const GRANTED_SECTION = "Подключено другими узлами";

export function grantedBy(name: string): string {
  return `Подключено узлом «${name}»`;
}

export const SCENARIO_LOAD_FAILED = "Не удалось загрузить сценарий.";
export const SCENARIO_ACTION_FAILED = "Не удалось выполнить операцию со сценарием.";
export const SCENARIO_MISSING = "Сценарий не найден — возможно, он уже удалён.";
export const DRAFT_SAVE_FAILED = "Черновик не сохранён. Изменения останутся в редакторе.";

export const CONNECT_REFUSALS: Record<ConnectRefusal, string> = {
  selfLoop: "Узел нельзя соединить с самим собой",
  notTrunk: "Основной связью соединяются только шаги сценария, а не подузлы",
  intoTrigger: "Триггер запускает сценарий, входящая связь к нему невозможна",
  duplicate: "Такая связь уже есть",
  cycle: "Связь замкнула бы цикл",
};

export function slotLabel(slot: ScenarioSlot, required: boolean): string {
  return required ? `${SLOT_LABELS[slot]} *` : SLOT_LABELS[slot];
}

function fillsText(fills: readonly ScenarioSlot[]): string {
  if (fills.length === 0) return "не подключается к слотам";
  return `подключается к слоту ${fills.map((slot) => `«${SLOT_LABELS[slot]}»`).join(", ")}`;
}

export function attachRefusalText(
  refusal: Extract<Refusal, { kind: "attach" }>,
  nameOf: (id: string) => string,
  fillsOf: (id: string) => readonly ScenarioSlot[],
): string {
  const child = `«${nameOf(refusal.childId)}»`;
  const parent = `«${nameOf(refusal.parentId)}»`;
  const slot = `«${SLOT_LABELS[refusal.slot]}»`;
  const reasons: Record<AttachRefusal, string> = {
    self: "Узел нельзя подключить к самому себе",
    unknownType: `Тип узла ${child} не зарегистрирован`,
    noSlot: `У узла ${parent} нет слота ${slot}`,
    wrongKind: `Узел ${child} не подходит для слота ${slot}: он ${fillsText(fillsOf(refusal.childId))}`,
    occupied: `Слот ${slot} узла ${parent} уже занят`,
    hasEdges: `Узел ${child} соединён основными связями — удалите их перед подключением к слоту`,
    cycle: `Узел ${child} уже содержит ${parent} среди своих подузлов`,
    sameKind: `В слоте ${slot} узла ${parent} уже есть узел этого вида`,
  };
  return reasons[refusal.reason];
}

export function refusalText(
  refusal: Refusal,
  nameOf: (id: string) => string,
  fillsOf: (id: string) => readonly ScenarioSlot[],
): string {
  return refusal.kind === "connect"
    ? CONNECT_REFUSALS[refusal.reason]
    : attachRefusalText(refusal, nameOf, fillsOf);
}

export function issueText(issue: ScenarioIssueDto, nameOf: (id: string) => string): string {
  const node = issue.nodeId === undefined ? "" : `«${nameOf(issue.nodeId)}»`;
  const slot = issue.slot === undefined ? "" : `«${SLOT_LABELS[issue.slot]}»`;
  const texts: Record<ScenarioIssueRule, string> = {
    duplicateNode: `Идентификатор узла ${node} повторяется`,
    unknownNodeType: `Тип узла ${node} не зарегистрирован`,
    unknownNode: `Связь ведёт к отсутствующему узлу ${node}`,
    duplicateEdge: `Связь из узла ${node} повторяется`,
    invalidEdge: `Узел ${node} нельзя соединять основной связью`,
    cycle: `Узел ${node} входит в цикл`,
    unsupportedSlot: `У узла ${node} нет слота ${slot}`,
    slotMismatch: `Узел ${node} не подходит для слота ${slot}`,
    multipleParents: `Узел ${node} подключён к нескольким узлам`,
    missingAttachment: `Слот ${slot} узла ${node} не заполнен`,
    tooManyAttachments: `Слот ${slot} узла ${node} принимает один узел`,
    missingParameter: `У узла ${node} не задан параметр «${issue.param ?? ""}»`,
    noTrigger: "В сценарии нет триггера",
    multipleTriggers: "В сценарии может быть только один триггер",
    unreachable: `Узел ${node} не достижим от триггера`,
    restrictedTool: `Инструмент «${issue.param ?? ""}» недоступен в наборе ${node}`,
    invalidPort: `У узла ${node} нет выхода «${issue.param ?? ""}»`,
    missingDialog: `Узлу ${node} нужен диалог: сценарий должен запускаться триггером «Сообщение в чате»`,
    limitedDialog: `Без триггера «Сообщение в чате» узел ${node} работает только с глобальной памятью`,
    duplicateKind: `Слот ${slot} узла ${node} принимает не больше одного узла каждого вида`,
  };
  return texts[issue.rule];
}

export function scenariosWord(count: number): string {
  return plural(count, "сценарий", "сценария", "сценариев");
}

export function stepsWord(count: number): string {
  return plural(count, "шаг", "шага", "шагов");
}

export function issuesWord(count: number): string {
  return plural(count, "ошибка", "ошибки", "ошибок");
}

export function storesWord(count: number): string {
  return plural(count, "хранилище", "хранилища", "хранилищ");
}

export function toolCountLabel(count: number): string {
  return plural(count, "инструмент", "инструмента", "инструментов");
}

const HOST_WORDED = new Set<AppErrorCode>([AppErrorCode.VALIDATION_FAILED]);

export function scenarioErrorCopy(
  error: unknown,
  fallback: string = SCENARIO_ACTION_FAILED,
): string {
  if (!isIpcError(error)) return fallback;
  if (HOST_WORDED.has(error.code) && error.message.length > 0) return error.message;
  if (error.code === AppErrorCode.NOT_FOUND) return SCENARIO_MISSING;
  return fallback;
}

const IssueList = ScenarioIssueDto.array();

export function refusalIssues(error: unknown): ScenarioIssueDto[] {
  if (!isIpcError(error)) return [];
  const parsed = IssueList.safeParse(error.details?.issues);
  return parsed.success ? parsed.data : [];
}

export function savedAtLabel(timestamp: number): string {
  return new Date(timestamp).toLocaleTimeString("ru-RU", { hour: "2-digit", minute: "2-digit" });
}
