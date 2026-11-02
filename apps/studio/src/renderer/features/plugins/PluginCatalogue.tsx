import { mdiChevronRight, mdiPlus, mdiToyBrickOutline } from "@mdi/js";
import { observer } from "mobx-react-lite";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { useNavigate } from "react-router-dom";
import type { IntegrationKind } from "@zvs/shared";
import IconButton from "../../ui/atoms/buttons/IconButton";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import SegmentedControl from "../../ui/atoms/SegmentedControl";
import TextInput from "../../ui/atoms/TextInput";
import Alert from "../../ui/molecules/Alert";
import EmptyState from "../../ui/molecules/EmptyState";
import { EMPTY_MESSAGE } from "../../ui/atoms/SelectInput";
import PageHeader from "../../ui/organisms/PageHeader";
import PluginMark from "./PluginMark";
import {
  CATALOGUE_SUBTITLE,
  CATALOGUE_TABS,
  CATALOGUE_TITLE,
  CONNECT_ACCOUNT,
  PLUGIN_DESCRIPTIONS,
  PLUGIN_LABELS,
  SEARCH_PLACEHOLDER,
  accountsWord,
} from "./pluginPresentation";
import { pluginPath } from "./pluginRoutes";
import type PluginStore from "./PluginStore";

export const EMPTY_SEARCH_HINT = "По вашему запросу ничего не найдено.";
export const NOTHING_CONNECTED = "Подключённых плагинов нет";
export const NOTHING_CONNECTED_HINT = "Выберите плагин и подключите аккаунт.";

const PluginRow = observer(function PluginRow({
  store,
  kind,
  count,
}: {
  readonly store: PluginStore;
  readonly kind: IntegrationKind;
  readonly count: number;
}) {
  const navigate = useNavigate();
  const label = PLUGIN_LABELS[kind];
  return (
    <li className="group relative flex items-center gap-3.5 rounded-card px-2.5 py-2.5 hover:bg-main-800">
      <button
        type="button"
        aria-label={label}
        className="absolute inset-0 rounded-card"
        onClick={() => {
          void navigate(pluginPath(kind));
        }}
      />
      <PluginMark kind={kind} size="md" className="pointer-events-none" />
      <span className="pointer-events-none flex min-w-0 flex-1 flex-col">
        <span className="truncate text-[14px] font-semibold text-main-50">{label}</span>
        <span className="truncate text-[12.5px] text-main-400">{PLUGIN_DESCRIPTIONS[kind]}</span>
      </span>
      {count === 0 ? (
        <span className="relative">
          <IconButton
            path={mdiPlus}
            label={CONNECT_ACCOUNT}
            size="md"
            iconSize={18}
            onClick={() => {
              store.startAdding(kind);
              void navigate(pluginPath(kind));
            }}
          />
        </span>
      ) : (
        <span className="pointer-events-none">
          <Chip tone="accent">{accountsWord(count)}</Chip>
        </span>
      )}
    </li>
  );
});

function PluginCatalogue({ store }: { readonly store: PluginStore }) {
  const searching = store.query.trim().length > 0;
  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col">
      <PageHeader icon={mdiToyBrickOutline} title={CATALOGUE_TITLE} subtitle={CATALOGUE_SUBTITLE} />

      <ScrollArea className="flex min-h-0 min-w-0 flex-1 flex-col">
        <div className="mx-auto flex w-full max-w-230 flex-col gap-5 px-4 pt-5 pb-12 sm:px-8">
          <TextInput
            preset="search"
            aria-label={SEARCH_PLACEHOLDER}
            placeholder={SEARCH_PLACEHOLDER}
            value={store.query}
            onChange={(event) => {
              store.setQuery(event.target.value);
            }}
          />

          <div>
            <SegmentedControl
              label={CATALOGUE_TITLE}
              value={store.tab}
              options={CATALOGUE_TABS}
              onChange={(tab) => {
                store.setTab(tab);
              }}
            />
          </div>

          {store.error === null ? null : (
            <Alert onDismiss={store.dismissError}>{store.error}</Alert>
          )}

          {store.catalogueEmpty && store.loaded ? (
            <div className="flex justify-center pt-6">
              <EmptyState
                icon={mdiToyBrickOutline}
                title={searching || store.tab === "all" ? EMPTY_MESSAGE : NOTHING_CONNECTED}
                description={
                  searching || store.tab === "all" ? EMPTY_SEARCH_HINT : NOTHING_CONNECTED_HINT
                }
              />
            </div>
          ) : (
            store.sections.map((section) => {
              const collapsed = store.isCollapsed(section.id);
              return (
                <section key={section.id} className="flex flex-col gap-2">
                  <h2 className="m-0">
                    <button
                      type="button"
                      aria-expanded={!collapsed}
                      onClick={() => {
                        store.toggleSection(section.id);
                      }}
                      className="flex items-center gap-1 rounded-control py-1 text-[16px] font-semibold tracking-[-0.01em] text-main-50 hover:text-main-200"
                    >
                      {section.label}
                      <Icon
                        path={mdiChevronRight}
                        size={18}
                        className={`text-main-500 transition-transform ${collapsed ? "" : "rotate-90"}`}
                      />
                    </button>
                  </h2>
                  {collapsed ? null : (
                    <ul className="m-0 grid list-none grid-cols-1 gap-x-8 gap-y-1 p-0 md:grid-cols-2">
                      {section.entries.map((entry) => (
                        <PluginRow
                          key={entry.kind}
                          store={store}
                          kind={entry.kind}
                          count={entry.count}
                        />
                      ))}
                    </ul>
                  )}
                </section>
              );
            })
          )}
        </div>
      </ScrollArea>
    </div>
  );
}

export default observer(PluginCatalogue);
