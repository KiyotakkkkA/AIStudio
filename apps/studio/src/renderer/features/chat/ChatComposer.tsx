import { useEffect, useRef, useState } from "react";
import { observer } from "mobx-react-lite";
import { Link } from "react-router-dom";
import { mdiClose, mdiCogOutline, mdiDatabaseOutline, mdiPaperclip, mdiUpload } from "@mdi/js";
import { Dropdown, InputCheckBox, Modal } from "@kiyotakkkka/zvs-uikit-lib";
import Button from "../../ui/atoms/buttons/Button";
import Icon from "../../ui/atoms/Icon";
import IconDropdownButton from "../../ui/atoms/buttons/IconDropdownButton";
import SelectInput from "../../ui/atoms/SelectInput";
import TextArea from "../../ui/atoms/TextArea";
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
    <div className="mx-auto w-full max-w-5xl shrink-0 px-5 pt-3 pb-4">
      {!store.available && !store.loading ? (
        <div className="rounded-card border border-main-750 bg-main-900 p-4 text-main-300">
          Выберите доступную текстовую модель, чтобы начать диалог.{" "}
          <Link className="text-accent-medium underline" to="/providers">
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
          className="space-y-3 rounded-xl border border-main-750 bg-main-900 p-3"
        >
          {store.attachedStoreIds.length > 0 && (
            <div className="flex flex-wrap gap-1.5">
              {store.stores
                .filter((item) => store.attachedStoreIds.includes(item.id))
                .map((item) => (
                  <span
                    key={item.id}
                    className="inline-flex h-8 items-center gap-1.5 rounded-lg bg-main-750 px-2.5 text-[12px] text-main-100"
                  >
                    <Icon path={mdiDatabaseOutline} size={16} className="text-main-300" />
                    <span className="max-w-40 truncate">{item.name}</span>
                    <button
                      type="button"
                      aria-label={`Отключить хранилище ${item.name}`}
                      className="text-main-500 hover:text-main-100"
                      onClick={() => void store.toggleStore(item.id)}
                    >
                      <Icon path={mdiClose} size={15} />
                    </button>
                  </span>
                ))}
            </div>
          )}
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
          <div className="flex flex-wrap items-center gap-2">
            <Dropdown menuWidth={300} menuPlacement="top-left">
              <IconDropdownButton
                label="Прикрепить файлы"
                path={mdiPaperclip}
                className="gap-0 p-0 size-8 justify-center"
                rounded="rounded-lg"
                iconSize={20}
              />
              <Dropdown.Menu
                role="menu"
                aria-label="Добавить контекст"
                rounded="rounded-md"
                className="rounded-card border-main-750"
              >
                <Dropdown.Item
                  role="menuitem"
                  rounded="rounded-md"
                  className="text-[12.5px] text-main-100"
                  onClick={() => setStoresOpen(true)}
                >
                  <span className="flex items-center gap-2">
                    <Icon path={mdiUpload} size={17} className="text-main-400" />
                    Загрузить с устройства
                  </span>
                </Dropdown.Item>
                <Dropdown.Item
                  role="menuitem"
                  rounded="rounded-md"
                  className="text-[12.5px] text-main-100"
                >
                  <span className="flex items-center gap-2">
                    <Icon path={mdiDatabaseOutline} size={17} className="text-main-400" />
                    Прдключить векторное хранилище
                  </span>
                </Dropdown.Item>
              </Dropdown.Menu>
            </Dropdown>
            <Dropdown menuWidth={300} menuPlacement="top-left">
              <IconDropdownButton
                label="Настройки провайдера и модели"
                path={mdiCogOutline}
                className="gap-0 p-0 size-8 justify-center"
                rounded="rounded-lg"
              />
              <Dropdown.Menu
                role="menu"
                aria-label="Провайдер и модель"
                rounded=""
                className="space-y-3 rounded-card border-main-750 bg-main-900 p-3"
              >
                <div>
                  <p className="mb-1.5 text-[11px] font-medium tracking-wide text-main-400 uppercase">
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
                  <p className="mb-1.5 text-[11px] font-medium tracking-wide text-main-400 uppercase">
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
            <span className="min-w-0 max-w-56 truncate text-sm text-main-400">
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
        className="w-lg max-w-[92vw] rounded-card border border-main-750 bg-main-900 p-4.5"
      >
        <h2 className="text-[16px] font-semibold text-main-50">Источники из хранилища</h2>
        <p className="mt-1 text-xs text-main-500">
          Перед ответом будут найдены релевантные фрагменты документов.
        </p>
        <div className="mt-4 space-y-1.5">
          {store.stores.map((item) => {
            const selected = store.attachedStoreIds.includes(item.id);
            return (
              <label
                key={item.id}
                className={`flex cursor-pointer items-start gap-3 rounded-card border p-3 ${
                  selected ? "border-accent-dark bg-main-750" : "border-main-800 bg-main-800"
                }`}
              >
                <InputCheckBox
                  aria-label={`Выбрать хранилище: ${item.name}`}
                  checked={selected}
                  disabled={!store.canConfigure}
                  onChange={() => void store.toggleStore(item.id)}
                />
                <span className="min-w-0">
                  <span className="block text-sm font-semibold text-main-100">{item.name}</span>
                  <span className="mt-1 block text-xs text-main-500">
                    {item.description || "Описание не задано"} · {item.documents} документов
                  </span>
                </span>
              </label>
            );
          })}
          {store.stores.length === 0 && (
            <p className="py-5 text-center text-xs text-main-500">Хранилища не подключены.</p>
          )}
        </div>
        <div className="mt-4 flex justify-end border-t border-main-750 pt-3">
          <Button type="button" tone="secondary" onClick={() => setStoresOpen(false)}>
            Готово
          </Button>
        </div>
      </Modal>
      <p className="mt-2 text-center text-[10.5px] text-main-500">
        Enter — отправить · Shift+Enter — новая строка
      </p>
    </div>
  );
});
