import { Modal } from "@kiyotakkkka/zvs-uikit-lib";
import { observer } from "mobx-react-lite";
import { ToolDefaultTier, ToolSource } from "@zvs/shared";
import Button from "../../ui/atoms/buttons/Button";
import SelectInput from "../../ui/atoms/SelectInput";
import { SOURCE_CLASS_LABELS } from "./toolFilters";
import type ToolStore from "./ToolStore";
import { DEFAULT_TIER_HINT, DEFAULT_TIER_LABELS } from "./toolPresentation";

const OPTIONS = ToolDefaultTier.options.map((value) => ({
  value,
  label: DEFAULT_TIER_LABELS[value],
}));

function PermissionDefaultsDialog({ store }: { readonly store: ToolStore }) {
  const draft = store.defaultsDraft;

  return (
    <Modal
      open={store.defaultsOpen}
      onClose={store.closeDefaults}
      label="Разрешения по умолчанию"
      className="w-130 max-w-[92vw] overflow-visible rounded-card border border-line bg-main-900 p-0 shadow-pop"
    >
      <div className="px-5 pt-5 pb-3">
        <h2 className="m-0 text-[15px] font-semibold tracking-[-0.01em] text-main-50">
          Разрешения по умолчанию
        </h2>
        <p className="m-0 mt-1 text-[13px] leading-relaxed text-main-400">
          Уровень, который получает инструмент при первой регистрации. Уже известные инструменты
          сохраняют своё разрешение — изменение здесь никогда не включит то, что вы выключили.
          Разрушающие инструменты всегда начинают с «off».
        </p>
      </div>

      <div className="flex flex-col gap-2.5 px-5 pb-4">
        {draft === null ? (
          <span className="text-[12px] text-main-500">Загрузка…</span>
        ) : (
          ToolSource.options.map((source) => (
            <div key={source} className="flex items-center gap-3">
              <span className="w-32 flex-none text-[13px] text-main-200">
                {SOURCE_CLASS_LABELS[source]}
              </span>
              <div className="min-w-0 flex-1">
                <SelectInput
                  label={`Разрешение по умолчанию: ${SOURCE_CLASS_LABELS[source]}`}
                  value={draft[source]}
                  options={OPTIONS}
                  onChange={(value) => {
                    const parsed = ToolDefaultTier.safeParse(value);
                    if (parsed.success) store.setDefault(source, parsed.data);
                  }}
                />
              </div>
            </div>
          ))
        )}
        <p className="m-0 text-[11.5px] text-main-500">{DEFAULT_TIER_HINT}</p>
      </div>

      <div className="flex justify-end gap-2 bg-main-850 px-5 py-3">
        <Button type="button" tone="secondary" onClick={store.closeDefaults}>
          Отмена
        </Button>
        <Button
          type="button"
          tone="primary"
          disabled={draft === null || store.savingDefaults}
          onClick={() => {
            void store.saveDefaults();
          }}
        >
          {store.savingDefaults ? "Сохранение…" : "Сохранить"}
        </Button>
      </div>
    </Modal>
  );
}

export default observer(PermissionDefaultsDialog);
