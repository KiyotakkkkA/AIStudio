import { observer } from "mobx-react-lite";
import { Floating } from "@kiyotakkkka/zvs-uikit-lib";
import { mdiWrenchOutline } from "@mdi/js";
import type { ToolTier } from "@zvs/shared";
import Icon from "../../ui/atoms/Icon";
import { TIER_HINTS, TIER_LABELS, plural } from "../connections/connectionPresentation";
import type ChatStore from "./ChatStore";
import { TIER_PILL } from "./ChatToolPicker";
import { isBuiltinTool, toolTitle } from "./toolCallCopy";
import { CORE_HINTS, offeredHint, toolBlockReason } from "./toolSets";

export const CORE_TOOLS_TITLE = "Базовые";
export const AUTOMATIC_TOOLS_TITLE = "Подключены автоматически";
export const MANUAL_TOOLS_TITLE = "Выбраны вручную";
export const TOOL_MISSING = "Инструмент не найден";
export const TOOLS_UNAVAILABLE = "Модель не поддерживает вызов инструментов";

function toolsCount(count: number): string {
  return plural(count, "инструмент", "инструмента", "инструментов");
}

function ToolName({ toolKey }: { readonly toolKey: string }) {
  const builtin = isBuiltinTool(toolKey);
  return (
    <span className="flex min-w-0 flex-1 items-baseline gap-1.5">
      <span
        className={`truncate text-[12px] font-medium text-main-100 ${builtin ? "" : "font-mono"}`}
      >
        {builtin ? toolTitle(toolKey) : toolKey}
      </span>
      {builtin && <span className="shrink-0 font-mono text-[10.5px] text-main-500">{toolKey}</span>}
    </span>
  );
}

function TierPill({ tier }: { readonly tier: ToolTier | null }) {
  if (tier === null) return null;
  return (
    <span
      title={TIER_HINTS[tier]}
      className={`shrink-0 rounded-full px-1.5 font-mono text-[10.5px]/4.5 ${TIER_PILL[tier]}`}
    >
      {TIER_LABELS[tier]}
    </span>
  );
}

interface Row {
  readonly key: string;
  readonly hint: string | null;
  readonly dimmed: string | null;
}

function Section({
  title,
  rows,
  tierOf,
  first,
}: {
  readonly title: string;
  readonly rows: readonly Row[];
  readonly tierOf: (key: string) => ToolTier | null;
  readonly first: boolean;
}) {
  if (rows.length === 0) return null;
  return (
    <div className={first ? "" : "mt-1 border-t border-line pt-1"}>
      <p className="px-2 pt-1 pb-1 text-[10.5px] font-semibold tracking-wide text-main-500 uppercase">
        {title}
      </p>
      <ul className="m-0 list-none p-0">
        {rows.map((row) => (
          <li
            key={row.key}
            title={row.dimmed ?? undefined}
            className={`flex flex-col rounded-control px-2 py-1 ${row.dimmed === null ? "" : "opacity-55"}`}
          >
            <span className="flex items-center gap-2">
              <ToolName toolKey={row.key} />
              <TierPill tier={tierOf(row.key)} />
            </span>
            {row.hint !== null && (
              <span className="truncate text-[11px] text-main-500">{row.hint}</span>
            )}
          </li>
        ))}
      </ul>
    </div>
  );
}

export default observer(function ChatToolsBadge({ store }: { readonly store: ChatStore }) {
  const core: Row[] = store.coreTools.map((key) => ({
    key,
    hint: CORE_HINTS[key] ?? null,
    dimmed: null,
  }));
  const automatic: Row[] = store.automaticTools.map((tool) => ({
    key: tool.key,
    hint: offeredHint(tool),
    dimmed: null,
  }));
  const manual: Row[] = store.enabledTools.map((key) => {
    const tool = store.tools.find((item) => item.key === key);
    return {
      key,
      hint: null,
      dimmed: tool === undefined ? TOOL_MISSING : toolBlockReason(tool),
    };
  });
  const total = core.length + automatic.length + manual.length;
  const unsupported = store.toolMode === "none";
  if (total === 0 && !unsupported) return null;
  const tierOf = (key: string) => store.tools.find((tool) => tool.key === key)?.tier ?? null;
  const sections = [
    { title: CORE_TOOLS_TITLE, rows: core },
    { title: AUTOMATIC_TOOLS_TITLE, rows: automatic },
    { title: MANUAL_TOOLS_TITLE, rows: manual },
  ].filter((section) => section.rows.length > 0);

  return (
    <Floating anchor="top-left">
      <Floating.Trigger>
        <span
          tabIndex={0}
          aria-label={toolsCount(total)}
          className={`inline-flex h-7 cursor-default items-center gap-1.5 rounded-full px-2.5 text-[12px] font-medium outline-none focus-visible:outline-2 focus-visible:outline-accent-dark ${
            unsupported
              ? "border border-dashed border-line-strong text-main-400"
              : "bg-accent-soft text-accent-medium"
          }`}
        >
          <Icon path={mdiWrenchOutline} size={13} />
          <span className="font-mono text-[11.5px] tabular-nums">{total}</span>
        </span>
      </Floating.Trigger>
      <Floating.Content className="w-80 rounded-card border border-line bg-main-900 p-1.5 shadow-pop">
        {unsupported && (
          <p className="px-2 pt-1 pb-1.5 text-[11.5px] text-warn">{TOOLS_UNAVAILABLE}</p>
        )}
        {sections.map((section, index) => (
          <Section
            key={section.title}
            title={section.title}
            rows={section.rows}
            tierOf={tierOf}
            first={index === 0}
          />
        ))}
      </Floating.Content>
    </Floating>
  );
});
