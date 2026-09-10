import type {
  AdapterFamily,
  AuthMode,
  ModelCapability,
  ProbeOutcomeDto,
  ProbeResultDto,
  ProviderCapability,
  ProviderKind,
  ProviderStatus,
  ProviderSummaryDto,
  SecretSummaryDto,
} from "@zvs/shared";
import type { StatusTone } from "../../ui/atoms/statusTone";

export const CAPABILITY_LABELS: Record<ProviderCapability, string> = {
  text: "Генерация текста",
  embedding: "Эмбеддинги",
  image: "Генерация изображений",
};

export const CAPABILITY_LIST_HEADINGS: Record<ProviderCapability, string> = {
  text: "Провайдеры генерации текста",
  embedding: "Провайдеры эмбеддингов",
  image: "Провайдеры изображений",
};

export const KIND_LABELS: Record<ProviderKind, string> = {
  ollama: "Ollama",
  openrouter: "OpenRouter",
  mistral: "Mistral",
  "openai-compatible": "Совместимый с OpenAI",
};

export const ADAPTER_LABELS: Record<AdapterFamily, string> = {
  "openai-compatible": "openai-compatible",
  "qwen-web": "qwen-web",
  "deepseek-web": "deepseek-web",
};

export const AUTH_MODE_LABELS: Record<AuthMode, string> = {
  api: "API-ключ",
  account: "Аккаунт",
};

export const MODEL_CAPABILITY_LABELS: Record<ModelCapability, string> = {
  tools: "инструменты",
  vision: "зрение",
  streaming: "стриминг",
  reasoning: "рассуждения",
  code: "код",
};

export const PARAMETER_LABELS = {
  temperature: "Температура",
  topK: "Top K",
  topP: "Top P",
  maxOutputTokens: "Бюджет ответа",
} as const;

const KIND_BASE_URLS: Record<ProviderKind, string> = {
  ollama: "https://ollama.com/api",
  openrouter: "https://openrouter.ai/api/v1",
  mistral: "https://api.mistral.ai/v1",
  "openai-compatible": "",
};

const ADAPTER_BASE_URLS: Partial<Record<AdapterFamily, string>> = {
  "qwen-web": "https://chat.qwen.ai/api",
  "deepseek-web": "https://chat.deepseek.com/api",
};

const KIND_SECRET_TYPES: Record<ProviderKind, readonly string[]> = {
  ollama: ["ollama", "custom"],
  openrouter: ["openrouter", "custom"],
  mistral: ["mistral", "custom"],
  "openai-compatible": ["ollama", "openrouter", "mistral", "custom"],
};

export function suggestedBaseUrl(kind: ProviderKind, adapter: AdapterFamily): string {
  return ADAPTER_BASE_URLS[adapter] ?? KIND_BASE_URLS[kind];
}

export function secretsForKind(
  secrets: readonly SecretSummaryDto[],
  kind: ProviderKind,
): SecretSummaryDto[] {
  const allowed = KIND_SECRET_TYPES[kind];
  return secrets.filter((secret) => allowed.includes(secret.type));
}

export function providerInitials(name: string, kind: ProviderKind): string {
  const words = name
    .split(/[\s\-_.]+/u)
    .map((word) => word.replace(/[^\p{L}\p{N}]/gu, ""))
    .filter((word) => word.length > 0);
  if (words.length >= 2) return `${first(words[0])}${first(words[1])}`.toUpperCase();
  const single = words[0] ?? kind;
  return single.slice(0, 2).toUpperCase();
}

export function statusTone(status: ProviderStatus): StatusTone {
  switch (status) {
    case "ok":
      return "ok";
    case "degraded":
    case "needs-relink":
      return "warn";
    case "failed":
      return "err";
    default:
      return "idle";
  }
}

export const STATUS_LABELS: Record<ProviderStatus, string> = {
  unknown: "Не проверялся",
  ok: "Подключён",
  degraded: "Работает с оговорками",
  failed: "Ошибка подключения",
  "needs-relink": "Нужна повторная привязка",
};

export function summaryLine(summary: ProviderSummaryDto): string {
  if (summary.statusDetail !== null && summary.statusDetail.length > 0) return summary.statusDetail;
  if (summary.status === "ok") {
    return `${STATUS_LABELS.ok} · ${modelsWord(summary.modelCount)}`;
  }
  return STATUS_LABELS[summary.status];
}

export function modelsWord(count: number): string {
  const tail = count % 100;
  const last = count % 10;
  if (tail >= 11 && tail <= 14) return `${String(count)} моделей`;
  if (last === 1) return `${String(count)} модель`;
  if (last >= 2 && last <= 4) return `${String(count)} модели`;
  return `${String(count)} моделей`;
}

export interface ProbeLine {
  readonly tone: StatusTone;
  readonly text: string;
  readonly detail: string | null;
  /** Set when the outcome is about a session the user has to renew, not a bad key. */
  readonly relink: boolean;
}

export function probeLine(outcome: ProbeOutcomeDto): ProbeLine {
  switch (outcome.kind) {
    case "ok":
      return {
        tone: "ok",
        text: `200 OK · ${String(outcome.latencyMs)} мс · ${modelsWord(outcome.models.length)}`,
        detail: outcome.live ? null : "список моделей курируемый, а не полученный от вендора",
        relink: false,
      };
    case "ok-empty":
      return {
        tone: "warn",
        text: `200 OK · ${String(outcome.latencyMs)} мс · моделей нет`,
        detail: "Провайдер отвечает, но не вернул ни одной модели.",
        relink: false,
      };
    case "auth-failed":
      return {
        tone: "err",
        text: "Ключ отклонён",
        detail: "Проверьте выбранный секрет — учётные данные не приняты.",
        relink: false,
      };
    case "account-not-linked":
      return {
        tone: "warn",
        text: "Аккаунт не привязан",
        detail: "Свяжите аккаунт вендора, чтобы запросы получили сессию.",
        relink: true,
      };
    case "session-expired":
      return {
        tone: "warn",
        text: "Сессия истекла — войдите заново",
        detail: "Ключ здесь ни при чём: истекла сессия браузера вендора.",
        relink: true,
      };
    case "unreachable":
      return { tone: "err", text: "Провайдер недоступен", detail: outcome.detail, relink: false };
    case "rate-limited":
      return {
        tone: "warn",
        text: "Слишком много запросов",
        detail:
          outcome.retryAfter === undefined
            ? "Провайдер ограничил частоту запросов."
            : `Провайдер просит повторить через ${String(outcome.retryAfter)} с.`,
        relink: false,
      };
    default:
      return { tone: "err", text: "Ошибка проверки", detail: outcome.detail, relink: false };
  }
}

export function checkedAgo(result: ProbeResultDto, now: number): string {
  const seconds = Math.max(0, Math.round((now - result.checkedAt) / 1000));
  if (seconds < 60) return "проверено только что";
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `проверено ${String(minutes)} мин назад`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `проверено ${String(hours)} ч назад`;
  return `проверено ${String(Math.round(hours / 24))} дн. назад`;
}

export function contextWindowLabel(tokens: number): string {
  if (tokens >= 1000) return `${String(Math.round(tokens / 1000))}k ctx`;
  return `${String(tokens)} ctx`;
}

export function sizeLabel(bytes: number): string {
  const gb = bytes / 1024 ** 3;
  if (gb >= 1) return `${String(Math.round(gb))} ГБ`;
  const mb = bytes / 1024 ** 2;
  return `${String(Math.round(mb))} МБ`;
}

function first(word: string | undefined): string {
  return word === undefined ? "" : word.slice(0, 1);
}
