import { mdiWrenchOutline } from "@mdi/js";
import { observer } from "mobx-react-lite";
import { useEffect } from "react";
import TextInput from "../../ui/atoms/TextInput";
import PageHeader from "../../ui/organisms/PageHeader";
import { useStore } from "../../stores/useStore";
import ToolDetailRail from "./ToolDetailRail";
import ToolFilterRail from "./ToolFilterRail";
import ToolTable from "./ToolTable";
import Alert from "../../ui/molecules/Alert";

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
        subtitle="Доступные действия и правила использования инструментов"
        actions={
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
        }
      />

      {store.error === null ? null : (
        <Alert className="mx-6 mt-3" onDismiss={store.dismissError}>
          {store.error}
        </Alert>
      )}

      <div className="flex min-h-0 flex-1">
        <ToolFilterRail store={store} />
        <ToolTable store={store} />
        <ToolDetailRail store={store} />
      </div>
    </div>
  );
}

export default observer(ToolsWorkspace);
