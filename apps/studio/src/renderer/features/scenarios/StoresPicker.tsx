import { InputCheckBox } from "@kiyotakkkka/zvs-uikit-lib";
import { observer } from "mobx-react-lite";
import type { ScenarioNodeDto } from "@zvs/shared";
import { ENTITY_MISSING } from "./entityParams";
import { storeIds, toggleStore } from "./nodeRules";
import type ScenarioEditor from "./ScenarioEditor";
import { STORES_NONE } from "./scenarioCopy";

function StoresPicker({
  editor,
  node,
  invalid,
}: {
  readonly editor: ScenarioEditor;
  readonly node: ScenarioNodeDto;
  readonly invalid: boolean;
}) {
  const stores = editor.entities.options("vectorStore", node.params) ?? [];
  const chosen = storeIds(node.params);
  const missing = chosen.filter((id) => !stores.some((store) => store.value === id));
  const rows = [
    ...missing.map((id) => ({ value: id, label: id, missing: true })),
    ...stores.map((store) => ({ ...store, missing: false })),
  ];

  return (
    <div className={`rounded-card border bg-main-850 ${invalid ? "border-err" : "border-line"}`}>
      <div className="flex items-center gap-2 border-b border-line px-2.75 py-2">
        <span className="flex-1 text-[12px] font-medium text-main-100">Хранилища</span>
        <span className="font-mono text-[11px] text-main-500 tabular-nums">
          {chosen.length} / {stores.length}
        </span>
      </div>
      <div className="p-1.5">
        {rows.length === 0 ? (
          <p className="m-0 py-4 text-center text-[12px] text-main-500">{STORES_NONE}</p>
        ) : (
          rows.map((store) => {
            const selected = chosen.includes(store.value);
            return (
              <label
                key={store.value}
                className={`flex cursor-pointer items-center gap-2.5 rounded-control px-2 py-1.5 ${
                  store.missing ? "bg-err-soft" : selected ? "bg-accent-soft" : "hover:bg-main-800"
                }`}
              >
                <InputCheckBox
                  aria-label={`Подключить хранилище: ${store.label}`}
                  checked={selected}
                  onChange={() => {
                    editor.updateNode(node.id, { params: toggleStore(node.params, store.value) });
                  }}
                />
                <span className="min-w-0 flex-1">
                  <span
                    className={`block truncate text-[12px] font-medium ${
                      store.missing
                        ? "font-mono text-err"
                        : selected
                          ? "text-accent-medium"
                          : "text-main-100"
                    }`}
                  >
                    {store.label}
                  </span>
                  {store.missing ? (
                    <span className="block truncate text-[11px] text-err">{ENTITY_MISSING}</span>
                  ) : null}
                </span>
              </label>
            );
          })
        )}
      </div>
    </div>
  );
}

export default observer(StoresPicker);
