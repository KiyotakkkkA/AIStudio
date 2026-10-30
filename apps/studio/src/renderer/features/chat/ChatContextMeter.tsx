import { observer } from "mobx-react-lite";
import { useEffect } from "react";
import { isBuiltinTool, toolTitle as builtinTitle } from "./toolCallCopy";
import { Dropdown, Tooltip } from "@kiyotakkkka/zvs-uikit-lib";
import type { ChatContextDto, ToolDto } from "@zvs/shared";
import { plural } from "../connections/connectionPresentation";
import { sourceGroups, sourceKey } from "../tools/toolFilters";
import type ChatStore from "./ChatStore";
import { MeterBar, StackedBar, percent, share } from "../../ui/molecules/StackedBar";

interface Segment {
  readonly key: string;
  readonly label: string;
  readonly tokens: number;
  readonly fill: string;
}

const MESSAGE_FORMS = ["сообщение", "сообщения", "сообщений"] as const;

function messagesWord(count: number): string {
  return plural(count, ...MESSAGE_FORMS);
}

export function compactTokens(value: number): string {
  if (value < 1000) return String(value);
  const [divisor, suffix] = value < 1_000_000 ? [1000, "k"] : [1_000_000, "M"];
  const scaled = value / divisor;
  return `${scaled >= 100 ? Math.round(scaled) : scaled.toFixed(1).replace(/\.0$/, "")}${suffix}`;
}

function segments(context: ChatContextDto): Segment[] {
  return [
    { key: "system", label: "Системный промпт", tokens: context.system, fill: "bg-accent-dark" },
    { key: "tools", label: "Инструменты", tokens: context.tools, fill: "bg-info" },
    { key: "history", label: "История", tokens: context.history, fill: "bg-ok" },
    {
      key: "reserved",
      label: "Резерв под ответ",
      tokens: context.reservedOutput,
      fill: "bg-main-500/45",
    },
  ];
}

function ContextRing({
  value,
  pressured,
}: {
  readonly value: number;
  readonly pressured: boolean;
}) {
  const radius = 6;
  const length = 2 * Math.PI * radius;
  const filled = value > 0 ? Math.max(length * 0.04, (length * value) / 100) : 0;
  return (
    <svg aria-hidden="true" viewBox="0 0 16 16" className="size-4 -rotate-90">
      <circle cx="8" cy="8" r={radius} fill="none" strokeWidth="2.25" className="stroke-main-700" />
      <circle
        cx="8"
        cy="8"
        r={radius}
        fill="none"
        strokeWidth="2.25"
        strokeLinecap="round"
        strokeDasharray={`${filled} ${length}`}
        className={`transition-[stroke-dasharray] duration-300 ${pressured ? "stroke-warn" : "stroke-accent-dark"}`}
      />
    </svg>
  );
}

function Row({
  label,
  hint,
  value,
  dot,
}: {
  readonly label: string;
  readonly hint?: string;
  readonly value: string;
  readonly dot?: string;
}) {
  return (
    <div className="flex items-center gap-2 py-1">
      {dot !== undefined ? <span className={`size-2 shrink-0 rounded-full ${dot}`} /> : null}
      <span className="min-w-0 flex-1 truncate text-main-300">
        {label}
        {hint ? <span className="text-main-500"> · {hint}</span> : null}
      </span>
      <span className="shrink-0 font-mono text-[11px] text-main-400">{value}</span>
    </div>
  );
}

function ToolUsage({
  store,
  context,
}: {
  readonly store: ChatStore;
  readonly context: ChatContextDto;
}) {
  const known = new Map(store.tools.map((tool) => [tool.key, tool]));
  const offered = context.toolItems.flatMap((item) => {
    const tool = known.get(item.key);
    return tool ? [{ tool, tokens: item.tokens }] : [];
  });
  const groups = sourceGroups(
    offered.map((item) => item.tool),
    store.servers,
  ).map((group) => {
    const items = offered.filter((item) => sourceKey(item.tool) === group.key);
    return { group, items, tokens: items.reduce((sum, item) => sum + item.tokens, 0) };
  });
  if (groups.length === 0) return null;
  return (
    <section className="space-y-1.5">
      <p className="text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
        Статистика по инструментам
      </p>
      {groups.map(({ group, items, tokens }) => (
        <div key={group.key} className="rounded-lg bg-main-850 px-2.5 py-2">
          <div className="flex items-center gap-2">
            <span className="min-w-0 flex-1 truncate font-medium text-main-100">
              {group.label}
              {group.source === "mcp" ? (
                <span className="ml-1.5 text-[11px] font-normal text-main-500">MCP</span>
              ) : null}
            </span>
            <span className="shrink-0 font-mono text-[11px] text-main-400">
              {compactTokens(tokens)}
            </span>
          </div>
          <ul className="mt-1.5 space-y-1">
            {items.map(({ tool, tokens: cost }) => (
              <li key={tool.key} className="flex items-center gap-2">
                <span
                  className="min-w-0 flex-1 truncate font-mono text-[11px] text-main-400"
                  title={toolHint(tool)}
                >
                  {isBuiltinTool(tool.key) ? builtinTitle(tool.key) : tool.key}
                </span>
                <MeterBar value={cost} total={context.contextWindow} fill="bg-info" />
                <span className="w-9 shrink-0 text-right font-mono text-[11px] text-main-500">
                  {percent(cost, context.contextWindow)}
                </span>
              </li>
            ))}
          </ul>
        </div>
      ))}
    </section>
  );
}

function toolHint(tool: ToolDto): string {
  return tool.displayName && tool.displayName !== tool.key
    ? `${tool.displayName} — ${tool.key}`
    : tool.key;
}

export default observer(function ChatContextMeter({ store }: { readonly store: ChatStore }) {
  const providerId = store.provider?.id;
  const openaiMode = store.openaiMode;
  const generating = store.generating;
  useEffect(() => {
    if (!generating) void store.refreshProviderUsage();
  }, [store, providerId, openaiMode, generating]);
  const context = store.context;
  const limit = store.contextWindow;
  const parts = context ? segments(context) : [];
  const committed = context ? context.total + context.reservedOutput : 0;
  const pressured = limit > 0 && committed / limit >= 0.85;
  const free = context ? Math.max(0, limit - committed) : 0;
  const last = store.lastTurn;
  const summary = `${compactTokens(store.contextUsed)} / ${limit > 0 ? compactTokens(limit) : "—"}`;

  return (
    <Dropdown menuWidth={340} menuPlacement="top-right">
      <Tooltip label="Контекстное окно" placement="top-center" className="">
        <Dropdown.Trigger
          onClick={() => {
            if (!store.usageLoading) void store.refreshProviderUsage();
          }}
          aria-label={`Контекстное окно: ${summary}`}
          icon={false}
          className={`h-8 w-auto justify-center gap-0 border-none cursor-auto bg-transparent px-2 hover:bg-main-800 ${
            pressured ? "text-warn" : "text-main-500 hover:text-main-200"
          }`}
        >
          <span className="flex items-center gap-1.5">
            <ContextRing value={share(store.contextUsed, limit)} pressured={pressured} />
            <span className="font-mono text-[11px] tabular-nums">{summary}</span>
          </span>
        </Dropdown.Trigger>
      </Tooltip>
      <Dropdown.Menu
        rounded="rounded-lg"
        className="max-h-[70vh] w-85 overflow-y-auto rounded-card border border-line bg-main-900 p-3.5 text-[12px] text-main-300 shadow-pop"
      >
        <div className="space-y-3.5">
          <div className="space-y-2">
            <div className="flex items-baseline gap-2">
              <p className="flex-1 text-[13px] font-semibold text-main-50">Контекстное окно</p>
              <span className="font-mono text-[11px] text-main-500">{summary}</span>
              <span
                className={`font-mono text-[12px] font-semibold ${pressured ? "text-warn" : "text-main-100"}`}
              >
                {percent(store.contextUsed, limit)}
              </span>
            </div>
            <StackedBar
              parts={parts.map((part) => ({ key: part.key, value: part.tokens, fill: part.fill }))}
              total={limit}
              className="h-2"
            />
            {context === null ? (
              <p className="text-[11.5px] text-main-500">Разбивка доступна после выбора модели.</p>
            ) : null}
          </div>

          {context !== null ? (
            <section>
              <div className="flex items-baseline justify-between">
                <p className="font-semibold text-main-100">Разбивка</p>
                <span className="max-w-40 truncate font-mono text-[11px] text-main-500">
                  {store.model?.displayName || store.model?.externalId}
                </span>
              </div>
              <div className="mt-1 divide-y divide-line">
                {parts.map((part) => (
                  <Row
                    key={part.key}
                    dot={part.fill}
                    label={part.label}
                    hint={
                      part.key === "tools" && context.toolItems.length > 0
                        ? String(context.toolItems.length)
                        : part.key === "history" && context.historyMessages > 0
                          ? messagesWord(context.historyMessages)
                          : undefined
                    }
                    value={compactTokens(part.tokens)}
                  />
                ))}
                <Row dot="bg-main-700" label="Свободно" value={compactTokens(free)} />
              </div>
              {context.trimmed > 0 ? (
                <p className="mt-2 rounded-lg bg-warn-soft px-2.5 py-1.5 text-[11.5px] text-warn">
                  {messagesWord(context.trimmed)} не помещаются и не попадут в следующий запрос.
                </p>
              ) : null}
            </section>
          ) : null}

          {context !== null ? <ToolUsage store={store} context={context} /> : null}

          {store.providerUsage !== null || store.accountFamily === "openai" ? (
            <section className="space-y-2">
              <p className="font-semibold text-main-100">Лимиты провайдера</p>
              {store.usageLoading ? (
                <p className="text-[11.5px] text-main-500">Загрузка лимитов…</p>
              ) : null}
              {store.providerUsage?.windows.map((window) => (
                <div key={window.label} className="space-y-1">
                  <Row
                    label={window.label}
                    value={`${window.usedPercent.toLocaleString("ru-RU")}%`}
                  />
                  <StackedBar
                    parts={[
                      {
                        key: "used",
                        value: window.usedPercent,
                        fill: window.usedPercent >= 85 ? "bg-warn" : "bg-accent-dark",
                      },
                    ]}
                    total={100}
                    className="h-1.5"
                  />
                  {window.resetsAt !== null ? (
                    <p className="text-[11px] text-main-500">
                      Обновление: {new Date(window.resetsAt).toLocaleString("ru-RU")}
                    </p>
                  ) : null}
                </div>
              ))}
              {store.providerUsage?.detail || store.providerUsageError ? (
                <p className="text-[11.5px] leading-snug text-main-500">
                  {store.providerUsageError ?? store.providerUsage?.detail}
                </p>
              ) : null}
              {store.accountFamily === "openai" ? (
                <button
                  type="button"
                  onClick={() => void store.openProviderUsage()}
                  className="text-[11.5px] text-accent-medium"
                >
                  Расход подписки в ChatGPT
                </button>
              ) : null}
            </section>
          ) : null}

          {last !== undefined ? (
            <section>
              <p className="font-semibold text-main-100">Последний ответ</p>
              <div className="mt-1 divide-y divide-line">
                <Row label="Вход" value={compactTokens(last.tokensIn)} />
                <Row label="Выход" value={compactTokens(last.tokensOut)} />
                <Row label="Всего за диалог" value={compactTokens(store.sessionTokens)} />
              </div>
            </section>
          ) : null}

          <p className="text-[11px] leading-snug text-main-500">
            Оценка по числу символов.
            {store.attachedStoreIds.length > 0
              ? " Фрагменты из хранилищ добавляются при отправке."
              : ""}
          </p>
        </div>
      </Dropdown.Menu>
    </Dropdown>
  );
});
