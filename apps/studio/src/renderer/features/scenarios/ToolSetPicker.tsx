import { mdiLockOutline, mdiMagnify } from "@mdi/js";
import { InputCheckBox } from "@kiyotakkkka/zvs-uikit-lib";
import { observer } from "mobx-react-lite";
import { useState } from "react";
import type { ScenarioNodeDto } from "@zvs/shared";
import Icon from "../../ui/atoms/Icon";
import { matchesQuery, sourceGroups, sourceKey } from "../tools/toolFilters";
import { grantedTools, isRestrictedTool } from "./nodeRules";
import { toolReason } from "./palette";
import { GRANTED_SECTION, grantedBy, TOOL_RESTRICTED } from "./scenarioCopy";
import type ScenarioEditor from "./ScenarioEditor";
import { toggleToolKey, toolSetKeys } from "./toolSet";

function ToolSetPicker({
  editor,
  node,
  invalid,
}: {
  readonly editor: ScenarioEditor;
  readonly node: ScenarioNodeDto;
  readonly invalid: boolean;
}) {
  const [query, setQuery] = useState("");
  const chosen = toolSetKeys(node.params);
  const offered = editor.tools.filter((tool) => !isRestrictedTool(tool.key));
  const visible = offered.filter((tool) => matchesQuery(tool, query));
  const granted = grantedTools(editor.graph, node.id);
  const groups = sourceGroups(visible, editor.servers).map((group) => ({
    group,
    items: visible
      .filter((tool) => sourceKey(tool) === group.key)
      .sort((left, right) => left.key.localeCompare(right.key)),
  }));
  const missing = chosen.filter(
    (key) => isRestrictedTool(key) || !editor.tools.some((tool) => tool.key === key),
  );

  return (
    <div className={`rounded-card border bg-main-850 ${invalid ? "border-err" : "border-line"}`}>
      <div className="flex items-center gap-2 border-b border-line px-2.75 py-2">
        <span className="flex-1 text-[12px] font-medium text-main-100">Инструменты</span>
        <span className="font-mono text-[11px] text-main-500 tabular-nums">
          {chosen.length} / {offered.length}
        </span>
      </div>
      <label className="mx-2 mt-2 flex h-8 items-center gap-2 rounded-control bg-main-800 px-2.5 text-main-500 focus-within:text-main-300">
        <Icon path={mdiMagnify} size={16} />
        <input
          aria-label="Поиск инструментов"
          placeholder="Поиск по названию или описанию"
          value={query}
          onChange={(event) => {
            setQuery(event.target.value);
          }}
          className="min-w-0 flex-1 bg-transparent text-[12.5px] text-main-100 outline-none placeholder:text-main-500"
        />
      </label>
      <div className="px-1.5 pb-1.5">
        {granted.length === 0 ? null : (
          <section className="pt-1">
            <p className="m-0 px-2 pt-1.5 pb-1 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
              {GRANTED_SECTION}
            </p>
            {granted.map((entry) => (
              <button
                key={`${entry.key}-${entry.nodeId}`}
                type="button"
                className="flex w-full items-center gap-2.5 rounded-control px-2 py-1.5 text-left hover:bg-main-800"
                onClick={() => {
                  editor.select(entry.nodeId);
                }}
              >
                <span className="flex size-4 flex-none items-center justify-center text-main-500">
                  <Icon path={mdiLockOutline} size={14} />
                </span>
                <span className="min-w-0 flex-1">
                  <span className="block truncate font-mono text-[12px] font-medium text-main-200">
                    {entry.key}
                  </span>
                  <span className="block truncate text-[11px] text-main-500">
                    {grantedBy(editor.nameOf(entry.nodeId))}
                  </span>
                </span>
              </button>
            ))}
          </section>
        )}
        {missing.map((key) => (
          <label
            key={key}
            className="mt-1 flex cursor-pointer items-center gap-2.5 rounded-control bg-err-soft px-2 py-1.5"
          >
            <InputCheckBox
              aria-label={`Убрать из набора: ${key}`}
              checked
              onChange={() => {
                editor.updateNode(node.id, { params: toggleToolKey(node.params, key) });
              }}
            />
            <span className="min-w-0 flex-1">
              <span className="block truncate font-mono text-[12px] text-err">{key}</span>
              <span className="block truncate text-[11px] text-err">
                {isRestrictedTool(key) ? TOOL_RESTRICTED : toolReason(key, editor.tools)}
              </span>
            </span>
          </label>
        ))}
        {groups.map(({ group, items }) => (
          <section key={group.key} className="pt-1">
            <p className="m-0 px-2 pt-1.5 pb-1 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
              {group.label}
            </p>
            {items.map((tool) => {
              const selected = chosen.includes(tool.key);
              const blocked = toolReason(tool.key, editor.tools);
              return (
                <label
                  key={tool.id}
                  className={`flex items-center gap-2.5 rounded-control px-2 py-1.5 ${
                    selected ? "bg-accent-soft" : ""
                  } ${blocked !== null && !selected ? "opacity-55" : "cursor-pointer hover:bg-main-800"}`}
                >
                  <InputCheckBox
                    aria-label={`Включить в набор: ${tool.key}`}
                    checked={selected}
                    disabled={blocked !== null && !selected}
                    onChange={() => {
                      editor.updateNode(node.id, { params: toggleToolKey(node.params, tool.key) });
                    }}
                  />
                  <span className="min-w-0 flex-1">
                    <span
                      className={`block truncate font-mono text-[12px] font-medium ${
                        selected ? "text-accent-medium" : "text-main-100"
                      }`}
                    >
                      {tool.key}
                    </span>
                    <span
                      title={blocked ?? tool.description}
                      className={`block truncate text-[11px] ${blocked === null ? "text-main-500" : "text-warn"}`}
                    >
                      {blocked ?? (tool.description || tool.displayName)}
                    </span>
                  </span>
                </label>
              );
            })}
          </section>
        ))}
        {visible.length === 0 ? (
          <p className="m-0 py-4 text-center text-[12px] text-main-500">Ничего не найдено</p>
        ) : null}
      </div>
    </div>
  );
}

export default observer(ToolSetPicker);
