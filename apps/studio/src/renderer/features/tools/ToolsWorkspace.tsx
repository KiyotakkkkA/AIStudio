import { mdiShieldLockOutline, mdiWrenchOutline } from "@mdi/js";
import { observer } from "mobx-react-lite";
import { useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { ROUTES } from "../../app/routes";
import Button from "../../ui/atoms/buttons/Button";
import Icon from "../../ui/atoms/Icon";
import TextInput from "../../ui/atoms/TextInput";
import PageHeader from "../../ui/organisms/PageHeader";
import { useStore } from "../../stores/useStore";
import { TOOLS_TAB_PARAM } from "../permissions/permissionsPresentation";
import ToolDetailRail from "./ToolDetailRail";
import ToolFilterRail from "./ToolFilterRail";
import { headerLine } from "./toolPresentation";
import ToolTable from "./ToolTable";
import Alert from "../../ui/molecules/Alert";
import { InlineSkeleton, PAGE_LOADING, awaitingData } from "../../ui/molecules/LoadingSkeleton";

function ToolsWorkspace() {
  const { tools: store } = useStore();
  const navigate = useNavigate();

  useEffect(() => {
    store.mount();
    return store.dispose;
  }, [store]);

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col">
      <PageHeader
        icon={mdiWrenchOutline}
        title="Инструменты"
        subtitle={
          awaitingData(store) ? (
            <InlineSkeleton label={PAGE_LOADING} className="w-40" />
          ) : (
            headerLine(store.tools.length, store.groups.length)
          )
        }
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
              tone="secondary"
              onClick={() => {
                void navigate(`${ROUTES.permissions}?tab=${TOOLS_TAB_PARAM}`);
              }}
            >
              <Icon path={mdiShieldLockOutline} size={15} />
              Разрешения по умолчанию
            </Button>
          </>
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
