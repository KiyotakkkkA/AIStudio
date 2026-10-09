import { useEffect, useRef, useState } from "react";
import { observer } from "mobx-react-lite";
import { Link } from "react-router-dom";
import {
  mdiClose,
  mdiCogOutline,
  mdiDatabaseOutline,
  mdiInformationOutline,
  mdiPaperclip,
  mdiUpload,
} from "@mdi/js";
import { Dropdown, InputCheckBox, Modal, ProgressBar } from "@kiyotakkkka/zvs-uikit-lib";
import Button from "../../ui/atoms/buttons/Button";
import Icon from "../../ui/atoms/Icon";
import IconDropdownButton from "../../ui/atoms/buttons/IconDropdownButton";
import SelectInput from "../../ui/atoms/SelectInput";
import TextArea from "../../ui/atoms/TextArea";
import Toggle from "../../ui/atoms/Toggle";
import type ChatStore from "./ChatStore";

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
        <div className="rounded-card border border-dashed border-main-600 bg-main-850 p-4 text-[13px] text-main-300">
          Выберите доступную текстовую модель, чтобы начать диалог.{" "}
          <Link
            className="font-medium text-accent-medium underline decoration-accent-line underline-offset-2 hover:decoration-accent-medium"
            to="/providers"
          >
            Открыть провайдеров ИИ
          </Link>
        </div>
      ) : (
        <form
          ref={formRef}
          onSubmit={(event) => {
            event.preventDefault();
            void store.send();
          }}
          onClick={(event) => {
            if ((event.target as HTMLElement).closest("button, a")) return;
            event.currentTarget.querySelector<HTMLTextAreaElement>("textarea")?.focus();
          }}
          className="space-y-2 rounded-[14px] border border-main-750 bg-main-900 p-2.5 shadow-pop"
        >
          <div className="flex items-center gap-2 rounded-control bg-main-850 py-1 pr-2.5 pl-1">
            <Dropdown menuWidth={320} menuPlacement="top-left">
              <IconDropdownButton
                label="Контекстное окно"
                path={mdiInformationOutline}
                className="size-6 justify-center gap-0 rounded-[6px] border-transparent bg-transparent p-0 text-main-500 hover:bg-main-700 hover:text-main-100"
                iconSize={15}
              />
              <Dropdown.Menu
                rounded="rounded-lg"
                className="w-80 rounded-card border border-main-750 bg-main-900 p-3.5 text-[12px] text-main-300 shadow-pop"
              >
                <div className="space-y-2.5">
                  <p className="text-[13px] font-semibold text-main-50">Контекстное окно</p>
                  <ProgressBar
                    value={store.sessionTokens}
                    max={store.model?.contextWindow || Math.max(1, store.sessionTokens)}
                    label="Общий контекст"
                    showValue={false}
                    variant="info"
                    className="w-full"
                    classNames={{
                      label: "text-[12px] text-main-400",
                      track: "h-1.5 rounded-full bg-main-700",
                      indicator: "rounded-full bg-accent-dark",
                    }}
                  />
                  <div className="flex justify-end font-mono text-[11px] text-main-500">
                    {store.sessionTokens.toLocaleString()} /{" "}
                    {store.model?.contextWindow?.toLocaleString() ?? "—"}
                  </div>
                </div>
              </Dropdown.Menu>
            </Dropdown>
            <ProgressBar
              value={store.sessionTokens}
              max={store.model?.contextWindow || Math.max(1, store.sessionTokens)}
              label="Общий контекст"
              showValue={false}
              variant="info"
              className="min-w-0 flex-1"
              classNames={{
                header: "hidden",
                track: "h-1 rounded-full bg-main-700",
                indicator: "rounded-full bg-accent-dark",
              }}
            />
            <span className="shrink-0 font-mono text-[11px] text-main-500">
              {store.sessionTokens.toLocaleString()} /{" "}
              {store.model?.contextWindow?.toLocaleString() ?? "—"}
            </span>
          </div>
          {store.attachedStoreIds.length > 0 && (
            <div className="flex flex-wrap gap-1.5 px-1">
              {store.stores
                .filter((item) => store.attachedStoreIds.includes(item.id))
                .map((item) => (
                  <span
                    key={item.id}
                    className="inline-flex h-7 items-center gap-1.5 rounded-full border border-accent-line bg-accent-soft pr-1.5 pl-2.5 text-[12px] font-medium text-accent-medium"
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
          <div className="flex flex-wrap items-center gap-1.5">
            <Dropdown menuWidth={300} menuPlacement="top-left">
              <IconDropdownButton
                label="Прикрепить файлы"
                path={mdiPaperclip}
                className="size-8 justify-center gap-0 border-transparent bg-transparent p-0 text-main-400 hover:bg-main-700 hover:text-main-100"
                rounded="rounded-lg"
                iconSize={18}
              />
              <Dropdown.Menu
                role="menu"
                aria-label="Добавить контекст"
                rounded="rounded-lg"
                className="rounded-card border-main-750 bg-main-900 p-1 shadow-pop"
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
            <Dropdown menuWidth={300} menuPlacement="top-left">
              <IconDropdownButton
                label="Настройки провайдера и модели"
                path={mdiCogOutline}
                className="size-8 justify-center gap-0 border-transparent bg-transparent p-0 text-main-400 hover:bg-main-700 hover:text-main-100"
                rounded="rounded-lg"
                iconSize={18}
              />
              <Dropdown.Menu
                role="menu"
                aria-label="Провайдер и модель"
                rounded=""
                className="space-y-3.5 rounded-card border-main-750 bg-main-900 p-3.5 shadow-pop"
              >
                <div>
                  <p className="mb-1.5 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
                    Провайдер
                  </p>
                  <SelectInput
                    label="Провайдер чата"
                    value={store.provider?.id ?? ""}
                    options={store.providers.map((provider) => ({
                      value: provider.id,
                      label: provider.name,
                    }))}
                    disabled={!store.canConfigure}
                    onChange={(id) => {
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
            <span className="inline-flex h-7 max-w-56 min-w-0 items-center truncate rounded-full border border-main-750 bg-main-850 px-2.5 font-mono text-[11.5px] text-main-400">
              {store.model?.displayName || store.model?.externalId || store.modelId}
            </span>
            <span className="flex-1" />
            {store.generating ? (
              <Button
                type="button"
                tone="danger"
                disabled={!store.run || store.stopping}
                onClick={() => void store.stop()}
              >
                {store.stopping ? "Остановка…" : "Стоп"}
              </Button>
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
        className="w-lg max-w-[92vw] overflow-hidden rounded-card border border-main-750 bg-main-900 p-0 shadow-pop"
      >
        <div className="px-5 pt-5">
          <h2 className="text-[16px] font-semibold tracking-[-0.01em] text-main-50">
            Источники из хранилища
          </h2>
          <p className="mt-1 text-[12.5px] text-main-500">
            Перед ответом будут найдены релевантные фрагменты документов.
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
                    ? "border-accent-line bg-accent-soft"
                    : "border-main-750 bg-main-900 hover:border-main-600"
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
            <p className="rounded-card border border-dashed border-main-600 py-6 text-center text-[12.5px] text-main-500">
              Хранилища не подключены.
            </p>
          )}
          <div className="rounded-card border border-main-750 bg-main-850 p-3.5">
            <Toggle
              id="chat-use-relations"
              checked={store.useRelations}
              disabled={!store.canConfigure}
              label="Использовать связи"
              onChange={() => void store.toggleRelations()}
            />
            <p className="mt-1.5 text-[12px] text-main-500">
              Документы, связанные с найденными на вкладке «Связи», добавляются в контекст ниже
              основных источников и помечаются как связанные.
            </p>
          </div>
        </div>
        <div className="mt-5 flex justify-end border-t border-main-750 bg-main-850 px-5 py-3">
          <Button type="button" tone="secondary" onClick={() => setStoresOpen(false)}>
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
