import { observer } from "mobx-react-lite";
import { Link } from "react-router-dom";
import { mdiWrenchOutline } from "@mdi/js";
import { Dropdown, InputCheckBox, ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import type { ToolDto } from "@zvs/shared";
import { ROUTES } from "../../app/routes";
import IconDropdownButton from "../../ui/atoms/buttons/IconDropdownButton";
import { TIER_HINTS, TIER_LABELS, TIER_TEXT } from "../connections/connectionPresentation";
import type ChatStore from "./ChatStore";

export const TOOL_MODE_HINTS = {
  native: "Модель вызывает инструменты через API провайдера.",
  injected: "Инструменты описываются в системном промпте, модель отвечает JSON-вызовом.",
  none: "Эта модель не поддерживает вызов инструментов: они не будут предложены.",
} as const;

export function toolBlockReason(tool: ToolDto): string | null {
  if (!tool.available) return "Источник недоступен";
  if (!tool.enabled) return "Выключен в подключениях";
  if (tool.tier === "off") return "Запрещён на странице инструментов";
  return null;
}

export default observer(function ChatToolPicker({ store }: { readonly store: ChatStore }) {
  const count = store.enabledTools.length;
  return (
    <Dropdown menuWidth={360} menuPlacement="top-left">
      <span className="relative inline-flex">
        <IconDropdownButton
          label="Инструменты для модели"
          path={mdiWrenchOutline}
          className="size-8 justify-center gap-0 border-transparent bg-transparent p-0 text-main-400 hover:bg-main-700 hover:text-main-100"
          rounded="rounded-lg"
          iconSize={18}
        />
        {count > 0 && (
          <span
            aria-hidden="true"
            className="pointer-events-none absolute -top-0.5 -right-0.5 flex h-3.5 min-w-3.5 items-center justify-center rounded-full bg-accent-dark px-1 text-[9.5px] font-semibold text-on-accent tabular-nums"
          >
            {count}
          </span>
        )}
      </span>
      <Dropdown.Menu
        aria-label="Инструменты для модели"
        rounded=""
        className="rounded-card border-main-750 bg-main-900 p-3 shadow-pop"
      >
        <div className="mb-2.5 px-0.5">
          <p className="text-[13px] font-semibold text-main-50">Инструменты для модели</p>
          <p className="mt-0.5 text-[12px] text-main-500">{TOOL_MODE_HINTS[store.toolMode]}</p>
        </div>
        <ScrollArea className="max-h-72 space-y-1 pr-0.5">
          {store.selectableTools.map((tool) => {
            const selected = store.enabledTools.includes(tool.key);
            const blocked = toolBlockReason(tool);
            return (
              <label
                key={tool.id}
                className={`flex items-start gap-2.5 rounded-control border px-2.5 py-2 ${
                  selected ? "border-accent-line bg-accent-soft" : "border-transparent"
                } ${blocked && !selected ? "opacity-60" : "cursor-pointer hover:bg-main-850"}`}
              >
                <InputCheckBox
                  aria-label={`Предлагать модели: ${tool.key}`}
                  checked={selected}
                  disabled={!store.canConfigure || (blocked !== null && !selected)}
                  onChange={() => void store.toggleTool(tool.key)}
                />
                <span className="min-w-0 flex-1">
                  <span className="flex items-center gap-2">
                    <span className="min-w-0 truncate font-mono text-[12px] font-medium text-main-100">
                      {tool.key}
                    </span>
                    <span
                      title={TIER_HINTS[tool.tier]}
                      className={`shrink-0 font-mono text-[11px] ${TIER_TEXT[tool.tier]}`}
                    >
                      {TIER_LABELS[tool.tier]}
                    </span>
                    {tool.destructive && (
                      <span className="shrink-0 text-[11px] text-err">разрушающий</span>
                    )}
                  </span>
                  <span className="mt-0.5 line-clamp-2 block text-[11.5px] text-main-500">
                    {blocked ?? (tool.description || "Описание не задано")}
                  </span>
                </span>
              </label>
            );
          })}
          {store.selectableTools.length === 0 && (
            <p className="rounded-control border border-dashed border-main-600 py-5 text-center text-[12px] text-main-500">
              Инструменты не зарегистрированы.
            </p>
          )}
        </ScrollArea>
        <p className="mt-2.5 border-t border-main-750 px-0.5 pt-2.5 text-[11.5px] text-main-500">
          Разрешения задаются на{" "}
          <Link
            className="font-medium text-accent-medium underline decoration-accent-line underline-offset-2 hover:decoration-accent-medium"
            to={ROUTES.tools}
          >
            странице инструментов
          </Link>
          : auto — без подтверждения, ask — с подтверждением в чате, off — недоступен.
        </p>
      </Dropdown.Menu>
    </Dropdown>
  );
});
