import { observer } from "mobx-react-lite";
import { Floating } from "@kiyotakkkka/zvs-uikit-lib";
import { mdiWrenchOutline } from "@mdi/js";
import type { ChatContextToolDto, ToolTier } from "@zvs/shared";
import Icon from "../../ui/atoms/Icon";
import { TIER_HINTS, TIER_LABELS, plural } from "../connections/connectionPresentation";
import type ChatStore from "./ChatStore";
import { isBuiltinTool, toolTitle } from "./toolCallCopy";
import { SOURCE_HINTS, type ToolSection } from "./toolSets";

export const TOOLS_UNAVAILABLE = "Модель не поддерживает вызов инструментов";

export const TIER_PILL: Record<ToolTier, string> = {
  auto: "bg-ok-soft text-ok",
  ask: "bg-warn-soft text-warn",
  off: "bg-main-800 text-main-400",
};

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

function Section({
  section,
  tierOf,
  first,
}: {
  readonly section: ToolSection;
  readonly tierOf: (key: string) => ToolTier | null;
  readonly first: boolean;
}) {
  return (
    <div className={first ? "" : "mt-1 border-t border-line pt-1"}>
      <p className="px-2 pt-1 pb-1 text-[10.5px] font-semibold tracking-wide text-main-500 uppercase">
        {section.title}
      </p>
      <ul className="m-0 list-none p-0">
        {section.items.map((item: ChatContextToolDto) => {
          const hint = SOURCE_HINTS[item.source];
          return (
            <li key={item.key} className="flex flex-col rounded-control px-2 py-1">
              <span className="flex items-center gap-2">
                <ToolName toolKey={item.key} />
                <TierPill tier={tierOf(item.key)} />
              </span>
              {hint !== null && <span className="truncate text-[11px] text-main-500">{hint}</span>}
            </li>
          );
        })}
      </ul>
    </div>
  );
}

export default observer(function ChatToolsBadge({ store }: { readonly store: ChatStore }) {
  const sections = store.toolSections;
  const total = sections.reduce((sum, section) => sum + section.items.length, 0);
  const unsupported = store.toolMode === "none";
  if (total === 0 && !unsupported) return null;
  const tierOf = (key: string) => store.tools.find((tool) => tool.key === key)?.tier ?? null;

  return (
    <Floating anchor="top-right">
      <Floating.Trigger>
        <span
          tabIndex={0}
          aria-label={toolsCount(total)}
          className={`inline-flex h-7 cursor-default items-center gap-1.5 rounded-full px-2.5 text-[12px] font-medium text-main-400 outline-none hover:text-main-200 focus-visible:outline-2 focus-visible:outline-accent-dark ${
            unsupported ? "border border-dashed border-line-strong" : "bg-main-800"
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
          <Section key={section.title} section={section} tierOf={tierOf} first={index === 0} />
        ))}
      </Floating.Content>
    </Floating>
  );
});
