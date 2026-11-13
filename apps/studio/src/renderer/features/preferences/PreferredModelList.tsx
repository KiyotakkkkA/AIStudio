import { observer } from "mobx-react-lite";
import { mdiChevronDown, mdiChevronUp, mdiClose } from "@mdi/js";
import Button from "../../ui/atoms/buttons/Button";
import IconButton from "../../ui/atoms/buttons/IconButton";
import Chip from "../../ui/atoms/Chip";
import ModelFields, { availableModelOptions } from "../chat/ModelFields";
import {
  SUBAGENT_ADD,
  SUBAGENT_DOWN,
  SUBAGENT_MISSING_MODEL,
  SUBAGENT_MODEL,
  SUBAGENT_NONE,
  SUBAGENT_PROVIDER,
  SUBAGENT_REMOVE,
  SUBAGENT_UP,
} from "../subagents/subagentCopy";
import SettingRow from "./SettingRow";
import type { PreferredModelList as ModelList } from "./preferredModels";

export interface PreferredModelCopy {
  readonly row: { readonly key: string; readonly label: string; readonly hint: string };
  readonly empty: string;
  readonly noProviders: string;
  readonly providerLabel: string;
  readonly modelLabel: string;
}

export default observer(function PreferredModelList({
  list,
  copy,
}: {
  readonly list: ModelList;
  readonly copy: PreferredModelCopy;
}) {
  const empty = list.loaded && list.providers.length === 0;
  return (
    <SettingRow
      id={copy.row.key}
      label={copy.row.label}
      hint={copy.row.hint}
      error={list.error ?? undefined}
      stacked
      footer={
        empty ? <p className="m-0 text-[12px] text-main-500">{copy.noProviders}</p> : undefined
      }
    >
      {list.views.length === 0 ? (
        <p className="m-0 text-[12px] text-main-400">{copy.empty}</p>
      ) : (
        <ol className="m-0 flex list-none flex-col gap-1 p-0">
          {list.views.map((view, index) => (
            <li
              key={view.key}
              className="flex min-w-0 items-center gap-2 rounded-control border border-line bg-main-800 px-2.5 py-1.5"
            >
              <span className="w-4 flex-none font-mono text-[11.5px] text-main-500">
                {index + 1}
              </span>
              <span className="min-w-0 flex-1 truncate text-[12.5px] text-main-100">
                {view.modelName}
                <span className="text-main-500"> · {view.providerName}</span>
              </span>
              {view.missing ? <Chip>{SUBAGENT_MISSING_MODEL}</Chip> : null}
              <IconButton
                path={mdiChevronUp}
                label={SUBAGENT_UP}
                size="sm"
                disabled={index === 0}
                onClick={() => void list.move(index, -1)}
              />
              <IconButton
                path={mdiChevronDown}
                label={SUBAGENT_DOWN}
                size="sm"
                disabled={index === list.views.length - 1}
                onClick={() => void list.move(index, 1)}
              />
              <IconButton
                path={mdiClose}
                label={SUBAGENT_REMOVE}
                size="sm"
                onClick={() => void list.remove(index)}
              />
            </li>
          ))}
        </ol>
      )}
      {list.full ? null : (
        <div className="flex flex-wrap items-end gap-3">
          <div className="grid min-w-0 flex-1 grid-cols-2 gap-3 max-sm:grid-cols-1">
            <ModelFields
              providerTitle={SUBAGENT_PROVIDER}
              providerLabel={copy.providerLabel}
              providerValue={list.draftProviderId ?? ""}
              providerOptions={list.providers.map((provider) => ({
                value: provider.id,
                label: provider.name,
              }))}
              providerDisabled={list.providers.length === 0}
              onProviderChange={list.selectDraftProvider}
              modelTitle={SUBAGENT_MODEL}
              modelLabel={copy.modelLabel}
              modelValue={list.draftModelId ?? ""}
              modelOptions={availableModelOptions(list.draftProvider)}
              modelDisabled={list.draftProvider === undefined}
              onModelChange={list.setDraftModel}
              placeholder={SUBAGENT_NONE}
            />
          </div>
          <Button tone="ghost" disabled={!list.canAdd} onClick={() => void list.addDraft()}>
            {SUBAGENT_ADD}
          </Button>
        </div>
      )}
    </SettingRow>
  );
});
