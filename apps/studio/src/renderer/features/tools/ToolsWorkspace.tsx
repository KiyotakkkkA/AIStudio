import { mdiShieldLockOutline, mdiWrenchOutline } from "@mdi/js";
import { observer } from "mobx-react-lite";
import { useEffect } from "react";
import Button from "../../ui/atoms/buttons/Button";
import Icon from "../../ui/atoms/Icon";
import TextInput from "../../ui/atoms/TextInput";
import PageHeader from "../../ui/organisms/PageHeader";
import { useStore } from "../../stores/useStore";
import PermissionDefaultsDialog from "./PermissionDefaultsDialog";
import ToolDetailRail from "./ToolDetailRail";
import ToolFilterRail from "./ToolFilterRail";
import { headerLine } from "./toolPresentation";
import ToolTable from "./ToolTable";

function ToolsWorkspace() {
  const { tools: store } = useStore();

  useEffect(() => {
    store.mount();
    return store.dispose;
  }, [store]);

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col">
      <PageHeader
        icon={mdiWrenchOutline}
        title="Инструменты"
        subtitle={headerLine(store.tools.length, store.groups.length)}
        actions={
          <>
            <div className="w-57.5">
              <TextInput
                preset="search"
                aria-label="Поиск инструментов"
                placeholder="Поиск инструментов…"
                value={store.query}
                onChange={(event) => {
                  store.setQuery(event.currentTarget.value);
                }}
              />
            </div>
            <Button
              type="button"
              tone="secondary"
              onClick={() => {
                void store.openDefaults();
              }}
            >
              <Icon path={mdiShieldLockOutline} size={15} />
              Разрешения по умолчанию
            </Button>
          </>
        }
      />

      {store.error === null ? null : (
        <div
          role="alert"
          className="mx-6 mt-3 flex flex-none items-center gap-3 rounded-control bg-err-soft px-3 py-2.25 text-[12px] text-err"
        >
          <span className="flex-1">{store.error}</span>
          <button type="button" className="text-main-400" onClick={store.dismissError}>
            Скрыть
          </button>
        </div>
      )}

      <div className="flex min-h-0 flex-1">
        <ToolFilterRail store={store} />
        <ToolTable store={store} />
        <ToolDetailRail store={store} />
      </div>

      <PermissionDefaultsDialog store={store} />
    </div>
  );
}

export default observer(ToolsWorkspace);
