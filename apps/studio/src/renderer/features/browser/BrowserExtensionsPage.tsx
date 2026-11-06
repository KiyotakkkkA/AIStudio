import { observer } from "mobx-react-lite";
import { useEffect } from "react";
import { mdiDeleteOutline, mdiPuzzleOutline } from "@mdi/js";
import Button from "../../ui/atoms/buttons/Button";
import IconButton from "../../ui/atoms/buttons/IconButton";
import Icon from "../../ui/atoms/Icon";
import Toggle from "../../ui/atoms/Toggle";
import PreferenceContainer from "../../ui/molecules/PreferenceContainer";
import { useStore } from "../../stores/useStore";
import SettingRow from "../preferences/SettingRow";
import BrowserSubPage from "./BrowserSubPage";
import type { BrowserPage } from "./BrowserDataBlocks";
import { MANAGE } from "./browserDataCopy";
import {
  EXTENSION_OFF,
  EXTENSION_ON,
  EXTENSION_REMOVE,
  EXTENSION_REMOVE_TITLE,
  EXTENSIONS_ADD,
  EXTENSIONS_EMPTY,
  EXTENSIONS_HINT,
  EXTENSIONS_ID,
  EXTENSIONS_LABEL,
  EXTENSIONS_TITLE,
  extensionRemoveConsequence,
} from "./browserExtensionsCopy";

export const ExtensionsBlock = observer(function ExtensionsBlock({
  onManage,
}: {
  readonly onManage: (page: BrowserPage) => void;
}) {
  return (
    <PreferenceContainer title={EXTENSIONS_TITLE}>
      <SettingRow id={EXTENSIONS_ID} label={EXTENSIONS_LABEL} hint={EXTENSIONS_HINT}>
        <Button tone="secondary" size="sm" onClick={() => onManage("extensions")}>
          {MANAGE}
        </Button>
      </SettingRow>
    </PreferenceContainer>
  );
});

export default observer(function BrowserExtensionsPage({
  onBack,
}: {
  readonly onBack: () => void;
}) {
  const { browserExtensions: extensions } = useStore();
  useEffect(() => {
    void extensions.load();
  }, [extensions]);
  return (
    <BrowserSubPage
      title={EXTENSIONS_LABEL}
      hint={EXTENSIONS_HINT}
      error={extensions.error}
      onDismissError={extensions.dismissError}
      onBack={onBack}
      actions={
        <Button
          tone="secondary"
          size="sm"
          disabled={extensions.busy}
          onClick={() => void extensions.add()}
        >
          {EXTENSIONS_ADD}
        </Button>
      }
    >
      <div className="rounded-card border border-line bg-main-850">
        {extensions.items.length === 0 ? (
          <p className="m-0 px-4 py-6 text-center text-[12.5px] text-main-500">
            {extensions.loaded ? EXTENSIONS_EMPTY : null}
          </p>
        ) : (
          <ul className="m-0 list-none p-0">
            {extensions.items.map((item) => (
              <li
                key={item.id}
                className="flex items-center gap-3 border-b border-line px-4 py-3 last:border-b-0"
              >
                <span className="flex size-9 flex-none items-center justify-center rounded-control bg-main-800 text-main-300">
                  <Icon path={mdiPuzzleOutline} size={18} />
                </span>
                <div className="flex min-w-0 flex-1 flex-col gap-0.5">
                  <span className="truncate text-[13px] font-medium text-main-100">
                    {item.name}
                    {item.version.length > 0 ? (
                      <span className="font-normal text-main-500"> · {item.version}</span>
                    ) : null}
                  </span>
                  {item.description.length > 0 ? (
                    <span className="truncate text-[12px] text-main-500">{item.description}</span>
                  ) : null}
                </div>
                <Toggle
                  label={item.enabled ? EXTENSION_ON : EXTENSION_OFF}
                  checked={item.enabled}
                  disabled={extensions.busy}
                  onChange={(value) => void extensions.setEnabled(item.id, value)}
                />
                <IconButton
                  path={mdiDeleteOutline}
                  label={EXTENSION_REMOVE}
                  tone="danger"
                  size="sm"
                  disabled={extensions.busy}
                  needConfirm
                  modalSetup={{
                    title: EXTENSION_REMOVE_TITLE,
                    content: extensionRemoveConsequence(item.name),
                    confirmLabel: EXTENSION_REMOVE,
                  }}
                  onClick={() => void extensions.remove(item.id)}
                />
              </li>
            ))}
          </ul>
        )}
      </div>
    </BrowserSubPage>
  );
});
