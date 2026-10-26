import { useState } from "react";
import { observer } from "mobx-react-lite";
import { Link } from "react-router-dom";
import { mdiArrowRight, mdiAutoFix, mdiMagnify, mdiWrenchOutline } from "@mdi/js";
import { Dropdown, InputCheckBox, ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import type { ToolDto, ToolTier } from "@zvs/shared";
import { ROUTES } from "../../app/routes";
import IconButton from "../../ui/atoms/buttons/IconButton";
import IconDropdownButton from "../../ui/atoms/buttons/IconDropdownButton";
import Icon from "../../ui/atoms/Icon";
import { TIER_HINTS, TIER_LABELS } from "../connections/connectionPresentation";
import { matchesQuery, sourceGroups, sourceKey } from "../tools/toolFilters";
import type ChatStore from "./ChatStore";
import { isBuiltinTool, toolTitle } from "./toolCallCopy";
import { offeredHint, toolBlockReason } from "./toolSets";

export const TOOLS_UNSUPPORTED =
  "Эта модель не поддерживает вызов инструментов: они не будут предложены.";
export const USE_RECOMMENDED = "Установить рекомендуемое";

export const TIER_PILL: Record<ToolTier, string> = {
  auto: "bg-ok-soft text-ok",
  ask: "bg-warn-soft text-warn",
  off: "bg-main-800 text-main-400",
};

const SEARCH_THRESHOLD = 6;

const ToolRow = observer(function ToolRow({
  store,
  tool,
}: {
  readonly store: ChatStore;
  readonly tool: ToolDto;
}) {
  const selected = store.enabledTools.includes(tool.key);
  const blocked = toolBlockReason(tool);
  const builtin = isBuiltinTool(tool.key);
  const name = builtin ? toolTitle(tool.key) : tool.key;
  const offered = store.automaticTools.find((item) => item.key === tool.key);
  const description =
    blocked ??
    (offered !== undefined ? offeredHint(offered) : tool.description || "Описание не задано");
  return (
    <label
      className={`flex items-center gap-2.5 rounded-control px-2 py-1.5 transition-colors ${
        selected ? "bg-accent-soft" : ""
      } ${blocked && !selected ? "opacity-55" : "cursor-pointer hover:bg-main-850"}`}
    >
      <InputCheckBox
        aria-label={`Предлагать модели: ${name}`}
        checked={selected}
        disabled={!store.canConfigure || (blocked !== null && !selected)}
        onChange={() => void store.toggleTool(tool.key)}
      />
      <span className="min-w-0 flex-1">
        <span className="flex items-center gap-1.5">
          <span
            className={`min-w-0 truncate text-[12px] font-medium ${builtin ? "" : "font-mono"} ${
              selected ? "text-accent-medium" : "text-main-100"
            }`}
          >
            {name}
          </span>
          {builtin && (
            <span className="shrink-0 font-mono text-[10.5px] text-main-500">{tool.key}</span>
          )}
          {tool.destructive && (
            <span className="shrink-0 rounded-full bg-err-soft px-1.5 text-[10.5px]/4  text-err">
              разрушающий
            </span>
          )}
        </span>
        <span
          title={description}
          className={`block truncate text-[11.5px] ${blocked ? "text-warn" : "text-main-500"}`}
        >
          {description}
        </span>
      </span>
      <span
        title={TIER_HINTS[tool.tier]}
        className={`shrink-0 rounded-full px-1.5 font-mono text-[10.5px]/4.5  ${TIER_PILL[tool.tier]}`}
      >
        {TIER_LABELS[tool.tier]}
      </span>
    </label>
  );
});

export default observer(function ChatToolPicker({ store }: { readonly store: ChatStore }) {
  const [query, setQuery] = useState("");
  const count = store.enabledTools.length;
  const tools = store.selectableTools;
  const visible = tools.filter((tool) => matchesQuery(tool, query));
  const groups = sourceGroups(visible, store.servers).map((group) => ({
    group,
    items: visible.filter((tool) => sourceKey(tool) === group.key),
  }));
  const unsupported = store.toolMode === "none";

  return (
    <Dropdown menuWidth={380} menuPlacement="top-left" onOpenChange={() => setQuery("")}>
      <IconDropdownButton
        label="Инструменты для модели"
        path={mdiWrenchOutline}
        iconSize={17}
        {...(count > 0
          ? {
              toneClassName: unsupported
                ? "bg-main-800 text-main-400 hover:bg-main-700"
                : "bg-accent-soft text-accent-medium hover:bg-accent-soft hover:brightness-110",
            }
          : {})}
      />
      <Dropdown.Menu
        aria-label="Инструменты для модели"
        rounded=""
        className="overflow-hidden rounded-card border-line bg-main-900 p-0 shadow-pop"
      >
        <div className="space-y-2.5 px-3 pt-3 pb-2">
          <div className="flex items-center gap-2">
            <p className="flex-1 text-[13px] font-semibold text-main-50">Инструменты</p>
            <span className="font-mono text-[11px] text-main-500 tabular-nums">
              {count} / {tools.length}
            </span>
            <IconButton
              path={mdiAutoFix}
              label={USE_RECOMMENDED}
              iconSize={16}
              disabled={
                !store.canConfigure ||
                store.recommendedTools.length === 0 ||
                store.usesRecommendedTools
              }
              onClick={() => void store.useRecommendedTools()}
            />
          </div>
          {unsupported && <p className="text-[11.5px] text-warn">{TOOLS_UNSUPPORTED}</p>}
          {tools.length > SEARCH_THRESHOLD && (
            <label className="flex h-8 items-center gap-2 rounded-control bg-main-800 px-2.5 text-main-500 focus-within:text-main-300">
              <Icon path={mdiMagnify} size={16} />
              <input
                aria-label="Поиск инструментов"
                placeholder="Поиск по названию или описанию"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                className="min-w-0 flex-1 bg-transparent text-[12.5px] text-main-100 outline-none placeholder:text-main-500"
              />
            </label>
          )}
        </div>
        <ScrollArea className="max-h-80 px-1.5 pb-1.5">
          {groups.map(({ group, items }) => (
            <section key={group.key} className="pt-1">
              <p className="flex items-center gap-2 px-2 pt-1.5 pb-1 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
                <span className="min-w-0 flex-1 truncate">{group.label}</span>
                <span className="font-mono tracking-normal tabular-nums">
                  {items.filter((tool) => store.enabledTools.includes(tool.key)).length}/
                  {items.length}
                </span>
              </p>
              <div className="space-y-0.5">
                {items.map((tool) => (
                  <ToolRow key={tool.id} store={store} tool={tool} />
                ))}
              </div>
            </section>
          ))}
          {tools.length === 0 && (
            <p className="mx-1.5 my-1 rounded-control bg-main-800 py-5 text-center text-[12px] text-main-500">
              Инструменты не зарегистрированы.
            </p>
          )}
          {tools.length > 0 && visible.length === 0 && (
            <p className="py-5 text-center text-[12px] text-main-500">Ничего не найдено.</p>
          )}
        </ScrollArea>
        <div className="flex items-center gap-3 bg-main-850 px-3 py-2 text-[11px] text-main-500">
          <span className="flex flex-1 flex-wrap gap-x-2.5 gap-y-0.5">
            {(["auto", "ask", "off"] as const).map((tier) => (
              <span key={tier} title={TIER_HINTS[tier]} className="inline-flex items-center gap-1">
                <span className={`rounded-full px-1.5 font-mono leading-4 ${TIER_PILL[tier]}`}>
                  {TIER_LABELS[tier]}
                </span>
                {tier === "auto" ? "сразу" : tier === "ask" ? "с подтверждением" : "запрещён"}
              </span>
            ))}
          </span>
          <Link
            className="inline-flex shrink-0 items-center gap-1 font-medium text-accent-medium hover:text-accent-light"
            to={ROUTES.tools}
          >
            Разрешения
            <Icon path={mdiArrowRight} size={13} />
          </Link>
        </div>
      </Dropdown.Menu>
    </Dropdown>
  );
});
