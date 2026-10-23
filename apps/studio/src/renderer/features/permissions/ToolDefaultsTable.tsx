import { observer } from "mobx-react-lite";
import { useEffect } from "react";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { ToolDefaultTier, ToolSource } from "@zvs/shared";
import { useStore } from "../../stores/useStore";
import SelectInput from "../../ui/atoms/SelectInput";
import { SOURCE_CLASS_LABELS } from "../tools/toolFilters";
import { DEFAULT_TIER_HINT, DEFAULT_TIER_LABELS } from "../tools/toolPresentation";
import {
  DEFAULTS_DESCRIPTION,
  DEFAULTS_SOURCE_COLUMN,
  DEFAULTS_TIER_COLUMN,
  DEFAULTS_TITLE,
} from "./permissionsPresentation";

const HEAD = "text-[10px] font-semibold tracking-[0.07em] text-main-500 uppercase";

const OPTIONS = ToolDefaultTier.options.map((value) => ({
  value,
  label: DEFAULT_TIER_LABELS[value],
}));

function ToolDefaultsTable() {
  const { permissions } = useStore();

  useEffect(() => {
    void permissions.loadDefaults();
  }, [permissions]);

  const defaults = permissions.defaults;

  return (
    <ScrollArea className="flex min-h-0 min-w-0 flex-1 flex-col">
      <section aria-label={DEFAULTS_TITLE} className="flex w-full flex-col gap-3 px-6 py-5">
        <div className="flex flex-col gap-1">
          <h2 className="m-0 text-[14px] font-semibold text-main-50">{DEFAULTS_TITLE}</h2>
          <p className="m-0 text-[12.5px] leading-relaxed text-main-400">{DEFAULTS_DESCRIPTION}</p>
        </div>
        <div className="overflow-visible rounded-card border border-line">
          <div className="flex items-center gap-4 border-b border-line bg-main-850 px-4 py-2.5">
            <span className={`flex-1 ${HEAD}`}>{DEFAULTS_SOURCE_COLUMN}</span>
            <span className={`w-64 flex-none ${HEAD}`}>{DEFAULTS_TIER_COLUMN}</span>
          </div>
          {ToolSource.options.map((source) => (
            <div
              key={source}
              className="flex items-center gap-4 border-b border-line px-4 py-2.5 last:border-b-0"
            >
              <span className="flex-1 text-[13px] text-main-100">
                {SOURCE_CLASS_LABELS[source]}
              </span>
              <div className="w-64 flex-none">
                <SelectInput
                  label={`${DEFAULTS_TITLE}: ${SOURCE_CLASS_LABELS[source]}`}
                  value={defaults?.[source] ?? "inferred"}
                  options={OPTIONS}
                  disabled={defaults === null || permissions.savingSource !== null}
                  onChange={(value) => {
                    const parsed = ToolDefaultTier.safeParse(value);
                    if (parsed.success) void permissions.setDefault(source, parsed.data);
                  }}
                  menuPlacement="bottom-right"
                />
              </div>
            </div>
          ))}
        </div>
        <p className="m-0 text-[11.5px] text-main-500">{DEFAULT_TIER_HINT}</p>
      </section>
    </ScrollArea>
  );
}

export default observer(ToolDefaultsTable);
