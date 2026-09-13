import { useEffect, useRef } from "react";
import { observer } from "mobx-react-lite";
import { Link } from "react-router-dom";
import { mdiCogOutline } from "@mdi/js";
import { Dropdown } from "@kiyotakkkka/zvs-uikit-lib";
import Button from "../../ui/atoms/Button";
import Icon from "../../ui/atoms/Icon";
import SelectInput from "../../ui/atoms/SelectInput";
import TextArea from "../../ui/atoms/TextArea";
import type ChatStore from "./ChatStore";

export default observer(function ChatComposer({ store }: { readonly store: ChatStore }) {
  const formRef = useRef<HTMLFormElement>(null);
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
        <div className="rounded-card border border-main-600 bg-main-900 p-4 text-main-300">
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
          className="space-y-3 rounded-xl border border-main-600 bg-main-900 p-3"
        >
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
            <Dropdown menuWidth={300} menuPlacement="bottom-left">
              <Dropdown.Trigger
                className="gap-0 p-0 size-8 justify-center"
                rounded="rounded-lg"
                aria-label="Настройки провайдера и модели"
                title="Настройки провайдера и модели"
                icon={<Icon path={mdiCogOutline} size={20} />}
              >
                <></>
              </Dropdown.Trigger>
              <Dropdown.Menu
                role="menu"
                aria-label="Провайдер и модель"
                rounded=""
                className="space-y-3 rounded-card border-main-600 bg-main-900 p-3"
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
      <p className="mt-2 text-center text-[10.5px] text-main-500">
        Enter — отправить · Shift+Enter — новая строка
      </p>
    </div>
  );
});
