import { useEffect, useState } from "react";
import { observer } from "mobx-react-lite";
import { InputDropZone } from "@kiyotakkkka/zvs-uikit-lib";
import { mdiArchiveOutline, mdiFileExportOutline, mdiFileImportOutline, mdiRefresh } from "@mdi/js";
import { useStore } from "../../stores/useStore";
import SettingsSubPage from "./SettingsSubPage";
import Button from "../../ui/atoms/buttons/Button";
import Icon from "../../ui/atoms/Icon";
import Alert from "../../ui/molecules/Alert";
import DataTransferStore from "./DataTransferStore";
import DataTransferContents from "./DataTransferContents";
import {
  DATA_ARCHIVE_DATE,
  DATA_DEPENDENCIES,
  DATA_DROP_HINT,
  DATA_DROP_TITLE,
  DATA_EMPTY,
  DATA_EXPORTED,
  DATA_EXPORT_ACTION,
  DATA_EXPORT_HINT,
  DATA_EXPORT_GUIDANCE,
  DATA_EXPORTING,
  DATA_EXPORT_TITLE,
  DATA_FILE_SELECTED,
  DATA_IMPORTED,
  DATA_IMPORT_ACTION,
  DATA_IMPORT_HINT,
  DATA_IMPORT_GUIDANCE,
  DATA_IMPORTING,
  DATA_IMPORT_TITLE,
  DATA_INSPECTING,
  DATA_LOADING,
  DATA_REFRESH,
  DATA_REMOVE_FILE,
  DATA_UPLOADING,
} from "./dataTransferCopy";

export default observer(function DataTransferSettings({
  importing,
  onBack,
}: {
  readonly importing: boolean;
  readonly onBack: () => void;
}) {
  const root = useStore();
  const [store] = useState(
    () =>
      new DataTransferStore(root.ipc, async () => {
        await Promise.allSettled([
          root.ui.restore(),
          root.appSettings.load(),
          root.profile.load(),
          root.providers.load(),
          root.connections.load(),
          root.plugins.load(),
          root.skills.load(),
          root.scenarios.load(),
          root.memory.load(),
          root.storage.load(true),
          root.vectorStores.load(),
        ]);
      }),
  );
  useEffect(() => {
    if (!importing) void store.load();
    return () => store.dispose();
  }, [store, importing]);
  const status = store.working
    ? importing
      ? DATA_IMPORTING
      : DATA_EXPORTING
    : store.uploading
      ? store.progress === 100
        ? DATA_INSPECTING
        : `${DATA_UPLOADING} ${store.progress.toLocaleString("ru-RU")}%`
      : store.loading
        ? DATA_LOADING
        : null;
  return (
    <SettingsSubPage
      title={importing ? DATA_IMPORT_TITLE : DATA_EXPORT_TITLE}
      hint={importing ? DATA_IMPORT_HINT : DATA_EXPORT_HINT}
      onBack={onBack}
      backDisabled={store.working}
    >
      <div className="mx-auto flex w-full max-w-240 flex-col gap-5">
        <Alert tone="info">{importing ? DATA_IMPORT_GUIDANCE : DATA_EXPORT_GUIDANCE}</Alert>
        {importing ? (
          <InputDropZone
            file={store.file}
            accept=".dbox"
            disabled={store.working}
            onChange={(file) => void store.choose(file)}
            emptyIcon={<Icon path={mdiArchiveOutline} size={32} />}
            selectedIcon={<Icon path={mdiArchiveOutline} size={32} />}
            fileIcon={<Icon path={mdiArchiveOutline} size={20} />}
            emptyTitle={DATA_DROP_TITLE}
            emptyDescription={DATA_DROP_HINT}
            selectedDescription={DATA_FILE_SELECTED}
            clearLabel={DATA_REMOVE_FILE}
            clearAllLabel={DATA_REMOVE_FILE}
            previewAlt={DATA_FILE_SELECTED}
            uploadedFileLabel={DATA_FILE_SELECTED}
          />
        ) : (
          <div className="flex justify-end">
            <Button tone="ghost" size="sm" disabled={store.busy} onClick={() => void store.load()}>
              <Icon path={mdiRefresh} />
              {DATA_REFRESH}
            </Button>
          </div>
        )}
        {store.error === null ? null : <Alert>{store.error}</Alert>}
        {status === null ? null : (
          <p role="status" aria-live="polite" className="m-0 text-[13px] text-main-300">
            {status}
          </p>
        )}
        {store.preview === null ? null : (
          <>
            {importing ? (
              <p className="m-0 text-[12px] text-main-500">
                {DATA_ARCHIVE_DATE}: {new Date(store.preview.createdAt).toLocaleString("ru-RU")}
              </p>
            ) : null}
            <DataTransferContents store={store} />
            {store.preview.blockedCategories?.some((category) => store.selected.has(category)) ? (
              <Alert>{DATA_DEPENDENCIES}</Alert>
            ) : null}
            {store.preview.categories.every((item) => item.count === 0) ? (
              <p className="m-0 text-[13px] text-main-400">{DATA_EMPTY}</p>
            ) : null}
            <div className="flex justify-end">
              <Button
                tone="primary"
                size="sm"
                disabled={!store.canSubmit}
                onClick={() => {
                  if (importing) void store.import();
                  else void store.export();
                }}
              >
                <Icon path={importing ? mdiFileImportOutline : mdiFileExportOutline} />
                {importing ? DATA_IMPORT_ACTION : DATA_EXPORT_ACTION}
              </Button>
            </div>
          </>
        )}
        {store.savedPath === null ? null : (
          <div role="status" className="flex flex-col gap-1 text-[13px] text-main-300">
            <p className="m-0">{DATA_EXPORTED}</p>
            <p className="m-0 text-[12px] text-main-500 wrap-break-word select-text">
              {store.savedPath}
            </p>
          </div>
        )}
        {store.imported ? (
          <p role="status" className="m-0 text-[13px] text-main-300">
            {DATA_IMPORTED}
          </p>
        ) : null}
      </div>
    </SettingsSubPage>
  );
});
