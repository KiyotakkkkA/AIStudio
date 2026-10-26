import { useEffect, useRef, useState } from "react";
import { observer } from "mobx-react-lite";
import {
  mdiAutoFix,
  mdiChevronDown,
  mdiClose,
  mdiDatabaseOutline,
  mdiPaperclip,
  mdiStop,
  mdiUpload,
  mdiWrenchOutline,
} from "@mdi/js";
import { Dropdown, InputCheckBox, Modal } from "@kiyotakkkka/zvs-uikit-lib";
import Button from "../../ui/atoms/buttons/Button";
import IconButton from "../../ui/atoms/buttons/IconButton";
import Icon from "../../ui/atoms/Icon";
import IconDropdownButton from "../../ui/atoms/buttons/IconDropdownButton";
import SelectInput from "../../ui/atoms/SelectInput";
import TextArea from "../../ui/atoms/TextArea";
import SegmentedControl from "../../ui/atoms/SegmentedControl";
import Toggle from "../../ui/atoms/Toggle";
import { RETRIEVAL_MODES, retrievalHint } from "./retrievalModes";
import ChatContextMeter from "./ChatContextMeter";
import ChatModelGate from "./ChatModelGate";
import ChatToolPicker from "./ChatToolPicker";
import SelectionChips from "./SelectionChips";
import type ChatStore from "./ChatStore";
import { offeredHint } from "./toolSets";

function insidePopup(target: HTMLElement, form: HTMLElement): boolean {
  for (
    let node = target.closest<HTMLElement>("[id]");
    node !== null && node !== form;
    node = node.parentElement?.closest<HTMLElement>("[id]") ?? null
  ) {
    if (form.querySelector(`[aria-controls="${CSS.escape(node.id)}"]`)) return true;
  }
  return false;
}

export default observer(function ChatComposer({ store }: { readonly store: ChatStore }) {
  const formRef = useRef<HTMLFormElement>(null);
  const [storesOpen, setStoresOpen] = useState(false);
  const wasGenerating = useRef(store.generating);

  useEffect(() => {
    if (wasGenerating.current && !store.generating) {
      formRef.current?.querySelector<HTMLTextAreaElement>("textarea")?.focus();
    }
    wasGenerating.current = store.generating;
  }, [store.generating]);

  return (
    <div className="mx-auto w-full max-w-4xl shrink-0 px-6 pt-2 pb-4">
      {!store.available && !store.loading ? (
        <ChatModelGate store={store} />
      ) : (
        <form
          ref={formRef}
          onSubmit={(event) => {
            event.preventDefault();
            void store.send();
          }}
          onClick={(event) => {
            const target = event.target as HTMLElement;
            if (target.closest("button, a, input, select, label")) return;
            if (insidePopup(target, event.currentTarget)) return;
            event.currentTarget.querySelector<HTMLTextAreaElement>("textarea")?.focus();
          }}
          className="relative space-y-2 rounded-[18px] border border-line-strong bg-main-900 p-2.5 shadow-pop"
        >
          {store.generating ? (
            <span
              aria-hidden="true"
              className="pointer-events-none absolute inset-x-6 top-0 h-px overflow-hidden"
            >
              <span className="block h-full w-2/5 animate-zvs-sweep bg-linear-to-r from-transparent via-accent-dark to-transparent" />
            </span>
          ) : null}
          {store.composer.selections.length > 0 ? (
            <div className="px-1">
              <SelectionChips
                selections={store.composer.selections}
                onRemove={store.composer.removeSelection}
              />
            </div>
          ) : null}
          {(store.attachedStoreIds.length > 0 ||
            store.enabledTools.length > 0 ||
            store.automaticTools.length > 0) && (
            <div className="flex flex-wrap gap-1.5 px-1">
              {store.stores
                .filter((item) => store.attachedStoreIds.includes(item.id))
                .map((item) => (
                  <span
                    key={item.id}
                    className="inline-flex h-7 items-center gap-1.5 rounded-full bg-accent-soft pr-1.5 pl-2.5 text-[12px] font-medium text-accent-medium"
                  >
                    <Icon path={mdiDatabaseOutline} size={14} />
                    <span className="max-w-40 truncate">{item.name}</span>
                    <button
                      type="button"
                      aria-label={`Отключить хранилище ${item.name}`}
                      className="flex size-4.5 items-center justify-center rounded-full hover:bg-accent-dark hover:text-on-accent"
                      onClick={() => void store.toggleStore(item.id)}
                    >
                      <Icon path={mdiClose} size={13} />
                    </button>
                  </span>
                ))}
              {store.enabledTools.map((key) => {
                const tool = store.tools.find((item) => item.key === key);
                const blocked =
                  tool === undefined || !tool.available || !tool.enabled || tool.tier === "off";
                return (
                  <span
                    key={key}
                    title={
                      blocked
                        ? "Инструмент сейчас недоступен и не будет предложен модели"
                        : store.toolMode === "none"
                          ? "Модель не поддерживает вызов инструментов"
                          : undefined
                    }
                    className={`inline-flex h-7 items-center gap-1.5 rounded-full border pr-1.5 pl-2.5 text-[12px] font-medium ${
                      blocked || store.toolMode === "none"
                        ? "border-dashed border-line-strong bg-transparent text-main-400"
                        : "border-transparent bg-accent-soft text-accent-medium"
                    }`}
                  >
                    <Icon path={mdiWrenchOutline} size={13} />
                    <span className="max-w-48 truncate font-mono text-[11.5px]">{key}</span>
                    <button
                      type="button"
                      aria-label={`Не предлагать инструмент ${key}`}
                      disabled={!store.canConfigure}
                      className="flex size-4.5 items-center justify-center rounded-full hover:bg-accent-dark hover:text-on-accent disabled:opacity-50"
                      onClick={() => void store.toggleTool(key)}
                    >
                      <Icon path={mdiClose} size={13} />
                    </button>
                  </span>
                );
              })}
              {store.automaticTools.map((tool) => (
                <span
                  key={tool.key}
                  title={offeredHint(tool)}
                  className="inline-flex h-7 items-center gap-1.5 rounded-full border border-transparent bg-main-800 pr-2.5 pl-2.5 text-[12px] font-medium text-main-300"
                >
                  <Icon path={mdiAutoFix} size={13} />
                  <span className="max-w-48 truncate font-mono text-[11.5px]">{tool.key}</span>
                </span>
              ))}
            </div>
          )}
          <div className="px-0.5">
            <TextArea
              aria-label="Сообщение"
              noBorder
              placeholder="Задайте вопрос…"
              value={store.composer.text}
              maxLength={100000}
              disabled={store.loading || store.generating}
              onChange={(event) => store.composer.setText(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
                  event.preventDefault();
                  void store.send();
                }
              }}
            />
          </div>
          <div className="flex flex-wrap items-center gap-1">
            <Dropdown menuWidth={300} menuPlacement="top-left">
              <IconDropdownButton
                label="Прикрепить файлы"
                tooltip="top-center"
                path={mdiPaperclip}
                className="size-8 justify-center gap-0 border-none cursor-auto bg-transparent p-0 text-main-400 hover:bg-main-700 hover:text-main-100"
                rounded="rounded-lg"
                iconSize={18}
              />
              <Dropdown.Menu
                role="menu"
                aria-label="Добавить контекст"
                rounded="rounded-lg"
                className="rounded-card border-line bg-main-900 p-1 shadow-pop"
              >
                <Dropdown.Item
                  role="menuitem"
                  rounded="rounded-lg"
                  className="text-[13px] text-main-100"
                >
                  <span className="flex items-center gap-2">
                    <Icon path={mdiUpload} size={17} className="text-main-400" />
                    Загрузить с устройства
                  </span>
                </Dropdown.Item>
                <Dropdown.Item
                  role="menuitem"
                  rounded="rounded-lg"
                  className="text-[13px] text-main-100"
                  onClick={() => setStoresOpen(true)}
                >
                  <span className="flex items-center gap-2">
                    <Icon path={mdiDatabaseOutline} size={17} className="text-main-400" />
                    Подключить векторное хранилище
                  </span>
                </Dropdown.Item>
              </Dropdown.Menu>
            </Dropdown>
            <ChatToolPicker store={store} />
            <span className="flex-1" />
            <ChatContextMeter store={store} />
            <Dropdown menuWidth={300} menuPlacement="top-right">
              <Dropdown.Trigger
                title="Провайдер и модель"
                rounded="rounded-lg"
                icon={<Icon path={mdiChevronDown} size={15} className="shrink-0 text-main-500" />}
                className="h-8 w-auto max-w-64 min-w-0 gap-1 border-none cursor-auto bg-transparent px-2 text-[12.5px] text-main-200 hover:bg-main-700 hover:text-main-50"
              >
                <span className="flex min-w-0 items-center gap-1.5">
                  {store.provider ? (
                    <span className="hidden shrink-0 text-main-500 sm:inline">
                      {store.provider.name}
                    </span>
                  ) : null}
                  <span className="truncate font-medium">
                    {store.model?.displayName || store.model?.externalId || store.modelId}
                  </span>
                </span>
              </Dropdown.Trigger>
              <Dropdown.Menu
                role="menu"
                aria-label="Провайдер и модель"
                rounded=""
                className="space-y-3.5 rounded-card border-line bg-main-900 p-3.5 shadow-pop"
              >
                <div>
                  <p className="mb-1.5 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
                    Провайдер
                  </p>
                  <SelectInput
                    label="Провайдер чата"
                    value={store.provider?.id ?? ""}
                    options={[
                      ...store.providers.map((provider) => ({
                        value: provider.id,
                        label: provider.name,
                      })),
                      ...store.accountChoices.map((choice) => ({
                        value: choice.value,
                        label: choice.label,
                      })),
                    ]}
                    disabled={!store.canConfigure || store.connecting}
                    onChange={(id) => {
                      const choice = store.accountChoices.find((item) => item.value === id);
                      if (choice) {
                        void store.connectAccount(choice.accountId);
                        return;
                      }
                      const provider = store.providers.find((item) => item.id === id);
                      if (provider) store.composer.selectProvider(provider);
                    }}
                  />
                </div>
                <div>
                  <p className="mb-1.5 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
                    Модель
                  </p>
                  <SelectInput
                    label="Модель чата"
                    value={store.model?.externalId ?? store.modelId}
                    options={(store.provider?.models ?? [])
                      .filter((model) => model.available)
                      .map((model) => ({
                        value: model.externalId,
                        label: model.displayName || model.externalId,
                      }))}
                    disabled={!store.canConfigure}
                    onChange={store.composer.setModel}
                  />
                </div>
              </Dropdown.Menu>
            </Dropdown>
            {store.generating ? (
              <IconButton
                tone="danger"
                size="md"
                path={mdiStop}
                label={store.stopping ? "Остановка…" : "Стоп"}
                disabled={!store.run || store.stopping}
                onClick={() => void store.stop()}
              />
            ) : (
              <Button type="submit" tone="primary" disabled={!store.canSend}>
                Отправить
              </Button>
            )}
          </div>
        </form>
      )}
      <Modal
        open={storesOpen}
        onClose={() => setStoresOpen(false)}
        label="Источники из хранилища"
        className="w-lg max-w-[92vw] overflow-hidden rounded-card border border-line bg-main-900 p-0 shadow-pop"
      >
        <div className="px-5 pt-5">
          <h2 className="text-[16px] font-semibold tracking-[-0.01em] text-main-50">
            Источники из хранилища
          </h2>
          <p className="mt-1 text-[12.5px] text-main-500">
            {retrievalHint(store.retrievalMode, store.toolMode)}
          </p>
        </div>
        <div className="mt-4 space-y-2 px-5">
          {store.stores.map((item) => {
            const selected = store.attachedStoreIds.includes(item.id);
            return (
              <label
                key={item.id}
                className={`flex cursor-pointer items-start gap-3 rounded-card border p-3.5 ${
                  selected
                    ? "border-transparent bg-accent-soft"
                    : "border-transparent bg-main-800 hover:bg-main-700"
                }`}
              >
                <InputCheckBox
                  aria-label={`Выбрать хранилище: ${item.name}`}
                  checked={selected}
                  disabled={!store.canConfigure}
                  onChange={() => void store.toggleStore(item.id)}
                />
                <span className="min-w-0">
                  <span className="block text-[13px] font-semibold text-main-100">{item.name}</span>
                  <span className="mt-0.5 block text-[12px] text-main-500">
                    {item.description || "Описание не задано"} · {item.documents} документов
                  </span>
                </span>
              </label>
            );
          })}
          {store.stores.length === 0 && (
            <p className="rounded-card bg-main-800 py-6 text-center text-[12.5px] text-main-500">
              Хранилища не подключены.
            </p>
          )}
          <div className="rounded-card bg-main-800 p-3.5">
            <span className="mb-2 block text-[12px] font-medium text-main-300">
              Поиск по знаниям
            </span>
            <SegmentedControl
              label="Поиск по знаниям"
              value={store.retrievalMode}
              options={RETRIEVAL_MODES}
              onChange={(mode) => void store.setRetrievalMode(mode)}
            />
          </div>
          <div className="rounded-card bg-main-800 p-3.5">
            <Toggle
              id="chat-use-relations"
              checked={store.useRelations}
              disabled={!store.canConfigure}
              label="Использовать связи"
              onChange={() => void store.toggleRelations()}
            />
            <p className="mt-1.5 text-[12px] text-main-500">
              Связанные документы добавляются в контекст после основных источников и помечаются как
              связанные.
            </p>
          </div>
        </div>
        <div className="mt-5 flex justify-end bg-main-850 px-5 py-3">
          <Button tone="secondary" onClick={() => setStoresOpen(false)}>
            Готово
          </Button>
        </div>
      </Modal>
      <p className="mt-2 text-center text-[11px] text-main-500 [&_kbd]:font-mono">
        Enter — отправить · Shift+Enter — новая строка
      </p>
    </div>
  );
});
