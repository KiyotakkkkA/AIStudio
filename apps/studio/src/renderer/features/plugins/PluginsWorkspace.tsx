import { mdiArrowLeft, mdiPlus } from "@mdi/js";
import { observer } from "mobx-react-lite";
import { useEffect } from "react";
import { useNavigate, useParams } from "react-router-dom";
import Button from "../../ui/atoms/buttons/Button";
import IconButton from "../../ui/atoms/buttons/IconButton";
import Icon from "../../ui/atoms/Icon";
import { PaneSkeleton } from "../../ui/organisms/PageSkeletons";
import { ROUTES } from "../../app/routes";
import { useStore } from "../../stores/useStore";
import AutomationRail from "./AutomationRail";
import PluginAccountList from "./PluginAccountList";
import PluginAccountPane from "./PluginAccountPane";
import PluginCatalogue from "./PluginCatalogue";
import PluginMark from "./PluginMark";
import {
  CATALOGUE_TITLE,
  CONNECT_ACCOUNT,
  PLUGIN_DESCRIPTIONS,
  PLUGIN_LABELS,
} from "./pluginPresentation";
import { parseKind } from "./pluginRoutes";
import type PluginStore from "./PluginStore";

export const BACK_TO_CATALOGUE = "К каталогу плагинов";

const PluginView = observer(function PluginView({ store }: { readonly store: PluginStore }) {
  const navigate = useNavigate();
  const kind = store.kind;
  if (kind === null) return null;

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col">
      <header className="flex h-header flex-none items-center gap-3 border-b border-line px-6">
        <IconButton
          path={mdiArrowLeft}
          label={BACK_TO_CATALOGUE}
          size="md"
          iconSize={17}
          tooltip="bottom-left"
          onClick={() => {
            void navigate(ROUTES.plugins);
          }}
        />
        <PluginMark kind={kind} size="lg" />
        <div className="flex min-w-0 flex-1 flex-col">
          <h1 className="m-0 truncate text-[17px]/6 font-semibold tracking-[-0.015em] text-main-50">
            {PLUGIN_LABELS[kind]}
          </h1>
          <span className="truncate text-[12px]/4 text-main-500">{PLUGIN_DESCRIPTIONS[kind]}</span>
        </div>
        <Button
          tone="primary"
          disabled={store.adding && store.accountsOf(kind).length > 0}
          onClick={() => {
            store.startAdding();
          }}
        >
          <Icon path={mdiPlus} size={15} />
          {CONNECT_ACCOUNT}
        </Button>
      </header>

      <div className="flex min-h-0 flex-1">
        {store.accountsOf(kind).length === 0 ? null : <PluginAccountList store={store} />}
        {store.loaded ? <PluginAccountPane store={store} /> : <PaneSkeleton />}
        <AutomationRail store={store} />
      </div>
    </div>
  );
});

function PluginsWorkspace() {
  const { plugins: store } = useStore();
  const navigate = useNavigate();
  const params = useParams<{ kind?: string; accountId?: string }>();
  const kind = parseKind(params.kind);
  const accountId = kind === null ? null : (params.accountId ?? null);
  const invalid = params.kind !== undefined && kind === null;
  const redirect = store.redirect;

  useEffect(() => {
    void store.mount();
    return store.dispose;
  }, [store]);

  useEffect(() => {
    if (invalid) {
      void navigate(ROUTES.plugins, { replace: true });
      return;
    }
    store.select(kind, accountId);
  }, [store, kind, accountId, invalid, navigate]);

  useEffect(() => {
    if (redirect === null) return;
    const target = store.consumeRedirect();
    if (target !== null) void navigate(target, { replace: true });
  }, [store, redirect, navigate]);

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col" aria-label={CATALOGUE_TITLE}>
      {kind === null ? <PluginCatalogue store={store} /> : <PluginView store={store} />}
    </div>
  );
}

export default observer(PluginsWorkspace);
