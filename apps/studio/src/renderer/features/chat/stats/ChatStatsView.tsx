import { useState, type ReactNode } from "react";
import { observer } from "mobx-react-lite";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { mdiCheck, mdiChevronDown, mdiChevronRight, mdiContentCopy } from "@mdi/js";
import type { ChatStatsDto, MessageDto } from "@zvs/shared";
import Button from "../../../ui/atoms/buttons/Button";
import IconButton from "../../../ui/atoms/buttons/IconButton";
import Chip from "../../../ui/atoms/Chip";
import Icon from "../../../ui/atoms/Icon";
import { MeterBar, StackedBar, percent } from "../../../ui/molecules/StackedBar";
import { LoadingStatus, SkeletonLines } from "../../../ui/molecules/LoadingSkeleton";
import { compactTokens } from "../ChatContextMeter";
import { isBuiltinTool, toolTitle } from "../toolCallCopy";
import { sourceGroups, sourceKey } from "../../tools/toolFilters";
import type ChatStore from "../ChatStore";
import {
  type SectionRow,
  type ToolGroup,
  chatHints,
  dialogTotals,
  notSent,
  sectionRows,
  toolCallCounts,
  turnRows,
} from "./chatStatsModel";
import { TurnChart, TurnTable } from "./TurnChart";
import {
  HINTS_EMPTY,
  HINTS_HEADING,
  HISTORY_EMPTY,
  HISTORY_HEADING,
  HISTORY_NOT_SENT,
  HISTORY_SOURCES,
  ROLE_LABELS,
  SECTIONS_HEADING,
  SECTION_COPY,
  SECTION_EMPTY,
  SKILLS_APPROXIMATE,
  SOURCE_TOKENIZER,
  STATS_FAILED,
  STATS_LOADING,
  STATS_NO_MODEL,
  STATS_RETRY,
  STATS_TITLE,
  STATUS_LABELS,
  TOOLS_DEFERRED,
  TOOLS_HEADING,
  TOOLS_UNUSED,
  TOTAL_CACHED,
  TOTAL_INPUT,
  TOTAL_OUTPUT,
  TOTAL_REASONING,
  TURNS_CHART,
  TURNS_EMPTY,
  TURNS_HEADING,
  TURNS_TABLE,
  WINDOW_FREE,
  WINDOW_HEADING,
  WINDOW_LIMIT,
  WINDOW_RESERVED,
  WINDOW_USED,
  callsWord,
  messagesWord,
  toolsWord,
} from "./chatStatsCopy";

const PREVIEW_CHARS = 140;

function toolLabel(key: string): string {
  return isBuiltinTool(key) ? toolTitle(key) : key;
}

function Card({
  title,
  aside,
  children,
}: {
  readonly title: string;
  readonly aside?: ReactNode;
  readonly children: ReactNode;
}) {
  return (
    <section className="rounded-card border border-line bg-main-900 p-4">
      <div className="mb-3 flex items-baseline gap-2">
        <h3 className="flex-1 text-[13px] font-semibold text-main-50">{title}</h3>
        {aside}
      </div>
      {children}
    </section>
  );
}

function Tile({
  label,
  value,
  hint,
}: {
  readonly label: string;
  readonly value: string;
  readonly hint?: string;
}) {
  return (
    <div className="rounded-lg bg-main-850 px-3 py-2">
      <p className="truncate text-[11px] text-main-500">{label}</p>
      <p className="mt-0.5 font-mono text-[15px] text-main-100 tabular-nums">{value}</p>
      {hint ? <p className="text-[11px] text-main-500">{hint}</p> : null}
    </div>
  );
}

function WindowCard({ stats }: { readonly stats: ChatStatsDto }) {
  const { context } = stats;
  const committed = context.total + context.reservedOutput;
  const free = Math.max(0, context.contextWindow - committed);
  return (
    <Card title={WINDOW_HEADING} aside={<Chip>{`${SOURCE_TOKENIZER} (${stats.tokenizer})`}</Chip>}>
      <StackedBar
        parts={[
          { key: "system", value: context.system, fill: "bg-accent-dark" },
          { key: "tools", value: context.tools, fill: "bg-info" },
          { key: "history", value: context.history, fill: "bg-ok" },
          { key: "reserved", value: context.reservedOutput, fill: "bg-main-500/45" },
        ]}
        total={context.contextWindow}
        className="h-2.5"
      />
      <ul className="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-[11px] text-main-400">
        {[
          ["bg-accent-dark", "Системный промпт", context.system],
          ["bg-info", "Инструменты", context.tools],
          ["bg-ok", "История", context.history],
          ["bg-main-500/45", WINDOW_RESERVED, context.reservedOutput],
        ].map(([fill, label, value]) => (
          <li key={String(label)} className="flex items-center gap-1.5">
            <span className={`size-2 rounded-full ${String(fill)}`} />
            {label} <span className="font-mono text-main-500">{compactTokens(Number(value))}</span>
          </li>
        ))}
      </ul>
      <div className="mt-3 grid grid-cols-2 gap-2 sm:grid-cols-4">
        <Tile
          label={WINDOW_USED}
          value={compactTokens(context.total)}
          hint={percent(context.total, context.contextWindow)}
        />
        <Tile label={WINDOW_LIMIT} value={compactTokens(context.contextWindow)} />
        <Tile label={WINDOW_RESERVED} value={compactTokens(context.reservedOutput)} />
        <Tile
          label={WINDOW_FREE}
          value={compactTokens(free)}
          hint={percent(free, context.contextWindow)}
        />
      </div>
    </Card>
  );
}

function SectionItem({ row, window }: { readonly row: SectionRow; readonly window: number }) {
  const [open, setOpen] = useState(false);
  const [copied, setCopied] = useState(false);
  const empty = row.tokens === 0;
  return (
    <li className="py-1.5">
      <button
        type="button"
        disabled={empty}
        aria-expanded={open}
        onClick={() => setOpen(!open)}
        className="flex w-full items-center gap-2 text-left disabled:cursor-default"
      >
        <Icon
          path={open ? mdiChevronDown : mdiChevronRight}
          size={14}
          className={empty ? "text-transparent" : "text-main-500"}
        />
        <span className={`min-w-0 flex-1 truncate ${empty ? "text-main-500" : "text-main-200"}`}>
          {row.label}
          {empty ? <span className="text-main-500"> · {SECTION_EMPTY}</span> : null}
        </span>
        {empty ? null : (
          <>
            <MeterBar value={row.tokens} total={window} fill="bg-accent-dark" className="w-24" />
            <span className="w-10 shrink-0 text-right font-mono text-[11px] text-main-500">
              {percent(row.tokens, window)}
            </span>
          </>
        )}
        <span className="w-12 shrink-0 text-right font-mono text-[11px] text-main-400">
          {compactTokens(row.tokens)}
        </span>
      </button>
      {open ? (
        <div className="relative mt-1.5 ml-5 rounded-lg bg-main-850">
          <IconButton
            label={SECTION_COPY}
            tooltip="bottom-left"
            path={copied ? mdiCheck : mdiContentCopy}
            iconSize={14}
            className="absolute top-1 right-1"
            onClick={() => {
              void navigator.clipboard
                ?.writeText(row.text)
                .then(() => setCopied(true))
                .catch(() => setCopied(false));
            }}
          />
          <pre className="max-h-80 overflow-auto p-3 pr-10 font-mono text-[11px] leading-relaxed whitespace-pre-wrap text-main-300">
            {row.text}
          </pre>
        </div>
      ) : null}
    </li>
  );
}

function SectionsCard({ stats }: { readonly stats: ChatStatsDto }) {
  const rows = sectionRows(stats);
  const skills = rows.some((row) => row.key === "skills" && row.tokens > 0);
  return (
    <Card title={SECTIONS_HEADING}>
      <ul className="divide-y divide-line text-[12px]">
        {rows.map((row) => (
          <SectionItem key={row.key} row={row} window={stats.context.contextWindow} />
        ))}
      </ul>
      {skills ? <p className="mt-2 text-[11px] text-main-500">{SKILLS_APPROXIMATE}</p> : null}
    </Card>
  );
}

function ToolsCard({
  stats,
  groups,
  calls,
}: {
  readonly stats: ChatStatsDto;
  readonly groups: readonly ToolGroup[];
  readonly calls: ReadonlyMap<string, number>;
}) {
  const { context } = stats;
  const grouped = new Set(groups.flatMap((group) => group.keys));
  const rest = context.toolItems.filter((item) => !grouped.has(item.key)).map((item) => item.key);
  const all: ToolGroup[] = [
    ...groups,
    ...(rest.length > 0 ? [{ key: "service", label: "Служебные", mcp: false, keys: rest }] : []),
  ];
  return (
    <Card
      title={TOOLS_HEADING}
      aside={
        <span className="font-mono text-[11px] text-main-500">
          {toolsWord(context.toolItems.length)} · {compactTokens(context.tools)}
        </span>
      }
    >
      <div className="space-y-2">
        {all.map((group) => {
          const items = context.toolItems.filter((item) => group.keys.includes(item.key));
          if (items.length === 0) return null;
          const tokens = items.reduce((sum, item) => sum + item.tokens, 0);
          return (
            <div key={group.key} className="rounded-lg bg-main-850 px-3 py-2">
              <div className="flex items-center gap-2 text-[12px]">
                <span className="min-w-0 flex-1 truncate font-medium text-main-100">
                  {group.label}
                  {group.mcp ? (
                    <span className="ml-1.5 text-[11px] font-normal text-main-500">MCP</span>
                  ) : null}
                </span>
                <span className="font-mono text-[11px] text-main-400">{compactTokens(tokens)}</span>
              </div>
              <ul className="mt-1.5 space-y-1">
                {[...items]
                  .sort((left, right) => right.tokens - left.tokens)
                  .map((item) => {
                    const count = calls.get(item.key) ?? 0;
                    return (
                      <li key={item.key} className="flex items-center gap-2 text-[11px]">
                        <span
                          className={`min-w-0 flex-1 truncate font-mono ${count === 0 ? "text-main-500" : "text-main-300"}`}
                          title={item.key}
                        >
                          {toolLabel(item.key)}
                        </span>
                        <span className="w-24 shrink-0 text-right text-main-500">
                          {count === 0 ? TOOLS_UNUSED : callsWord(count)}
                        </span>
                        <MeterBar
                          value={item.tokens}
                          total={context.contextWindow}
                          fill="bg-info"
                        />
                        <span className="w-10 shrink-0 text-right font-mono text-main-400">
                          {compactTokens(item.tokens)}
                        </span>
                      </li>
                    );
                  })}
              </ul>
            </div>
          );
        })}
      </div>
      {stats.deferred.length > 0 ? (
        <p className="mt-2 text-[11px] text-main-500" title={stats.deferred.join(", ")}>
          {TOOLS_DEFERRED}: {toolsWord(stats.deferred.length)}.
        </p>
      ) : null}
    </Card>
  );
}

function HistoryCard({
  stats,
  attached,
  onOpen,
}: {
  readonly stats: ChatStatsDto;
  readonly attached: boolean;
  readonly onOpen: (id: string) => void;
}) {
  const included = stats.history.filter((entry) => entry.status === "included");
  return (
    <Card
      title={HISTORY_HEADING}
      aside={
        <span className="font-mono text-[11px] text-main-500">
          {messagesWord(included.length)} · {compactTokens(stats.context.history)}
        </span>
      }
    >
      {stats.history.length === 0 ? (
        <p className="text-[12px] text-main-500">{HISTORY_EMPTY}</p>
      ) : (
        <ul className="max-h-120 divide-y divide-line overflow-y-auto text-[12px]">
          {stats.history.map((entry) => {
            const dropped = notSent(entry);
            const muted = entry.status !== "included";
            return (
              <li key={entry.messageId}>
                <button
                  type="button"
                  onClick={() => onOpen(entry.messageId)}
                  className="flex w-full items-start gap-2 py-1.5 text-left hover:bg-main-850"
                >
                  <span className="w-24 shrink-0 text-[11px] text-main-500">
                    {ROLE_LABELS[entry.role]}
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className={`line-clamp-2 ${muted ? "text-main-500" : "text-main-300"}`}>
                      {entry.text.slice(0, PREVIEW_CHARS) || "—"}
                    </span>
                    {dropped.length > 0 ? (
                      <span className="block text-[11px] text-main-500">
                        {HISTORY_NOT_SENT}: {dropped.join(", ")}
                      </span>
                    ) : null}
                  </span>
                  <Chip tone={entry.status === "included" ? "accent" : "neutral"}>
                    {STATUS_LABELS[entry.status]}
                  </Chip>
                  <span className="w-12 shrink-0 text-right font-mono text-[11px] text-main-400">
                    {compactTokens(entry.tokens)}
                  </span>
                </button>
              </li>
            );
          })}
        </ul>
      )}
      {attached ? <p className="mt-2 text-[11px] text-main-500">{HISTORY_SOURCES}</p> : null}
    </Card>
  );
}

function TurnsCard({ messages }: { readonly messages: readonly MessageDto[] }) {
  const [table, setTable] = useState(false);
  const rows = turnRows(messages);
  const totals = dialogTotals(rows);
  return (
    <Card
      title={TURNS_HEADING}
      aside={
        rows.length > 0 ? (
          <Button size="sm" onClick={() => setTable(!table)}>
            {table ? TURNS_CHART : TURNS_TABLE}
          </Button>
        ) : null
      }
    >
      {rows.length === 0 ? (
        <p className="text-[12px] text-main-500">{TURNS_EMPTY}</p>
      ) : (
        <div className="space-y-3">
          <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
            <Tile label={TOTAL_INPUT} value={compactTokens(totals.input)} />
            <Tile label={TOTAL_OUTPUT} value={compactTokens(totals.output)} />
            <Tile
              label={TOTAL_CACHED}
              value={compactTokens(totals.cached)}
              hint={percent(totals.cached, totals.input)}
            />
            <Tile label={TOTAL_REASONING} value={compactTokens(totals.reasoning)} />
          </div>
          {table ? <TurnTable rows={rows} /> : <TurnChart rows={rows} />}
        </div>
      )}
    </Card>
  );
}

function HintsCard({
  stats,
  messages,
  groups,
}: {
  readonly stats: ChatStatsDto;
  readonly messages: readonly MessageDto[];
  readonly groups: readonly ToolGroup[];
}) {
  const hints = chatHints({ stats, messages, groups, toolLabel });
  return (
    <Card title={HINTS_HEADING}>
      {hints.length === 0 ? (
        <p className="text-[12px] text-main-500">{HINTS_EMPTY}</p>
      ) : (
        <ul className="space-y-2">
          {hints.map((hint) => (
            <li key={hint.key} className="rounded-lg bg-main-850 px-3 py-2">
              <p className="text-[12.5px] font-medium text-main-100">{hint.title}</p>
              <p className="mt-0.5 text-xs/snug  text-main-400">{hint.detail}</p>
            </li>
          ))}
        </ul>
      )}
    </Card>
  );
}

export default observer(function ChatStatsView({
  store,
  onOpenMessage,
}: {
  readonly store: ChatStore;
  readonly onOpenMessage: (id: string) => void;
}) {
  const { stats, loading, failed } = store.stats;
  const messages = store.active?.messages ?? [];
  const known = new Map(store.tools.map((tool) => [tool.key, tool]));
  const offered = (stats?.context.toolItems ?? []).flatMap((item) => {
    const tool = known.get(item.key);
    return tool ? [tool] : [];
  });
  const groups: ToolGroup[] = sourceGroups(offered, store.servers).map((group) => ({
    key: group.key,
    label: group.label,
    mcp: group.source === "mcp",
    keys: offered.filter((tool) => sourceKey(tool) === group.key).map((tool) => tool.key),
  }));
  const calls = toolCallCounts(messages);
  return (
    <ScrollArea showScrollbar={false} className="min-h-0 flex-1 px-6 pt-5">
      <div className="mx-auto w-full max-w-4xl space-y-4 pb-6" aria-label={STATS_TITLE}>
        {stats === null ? (
          failed ? (
            <div className="flex items-center gap-3 rounded-card border border-line bg-main-900 p-4 text-[12.5px] text-main-300">
              <span className="flex-1">{STATS_FAILED}</span>
              <Button size="sm" onClick={() => void store.stats.load()}>
                {STATS_RETRY}
              </Button>
            </div>
          ) : loading ? (
            <LoadingStatus label={STATS_LOADING} className="space-y-4">
              <SkeletonLines count={6} />
            </LoadingStatus>
          ) : (
            <p className="py-10 text-center text-[13px] text-main-500">{STATS_NO_MODEL}</p>
          )
        ) : (
          <>
            <WindowCard stats={stats} />
            <SectionsCard stats={stats} />
            <ToolsCard stats={stats} groups={groups} calls={calls} />
            <HistoryCard
              stats={stats}
              attached={store.attachedStoreIds.length > 0}
              onOpen={onOpenMessage}
            />
            <TurnsCard messages={messages} />
            <HintsCard stats={stats} messages={messages} groups={groups} />
          </>
        )}
      </div>
    </ScrollArea>
  );
});
