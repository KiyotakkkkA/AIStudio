import { observer } from "mobx-react-lite";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { useStore } from "../../stores/useStore";
import Button from "../../ui/atoms/Button";
import TextInput from "../../ui/atoms/TextInput";
import SelectInput from "../../ui/atoms/SelectInput";
import Field from "../../ui/molecules/Field";
import TextArea from "../../ui/atoms/TextArea";
import Chip from "../../ui/atoms/Chip";

function VectorStoreForm() {
  const { vectorStores: store } = useStore();
  const vm = store.form;
  if (!vm) return null;
  const fields = [
    ["dimension", "Размерность"],
    ["chunkSize", "Размер чанка"],
    ["chunkOverlap", "Перекрытие"],
  ] as const;
  return (
    <form
      className="flex min-h-0 min-w-0 flex-1 flex-col"
      onSubmit={(e) => {
        e.preventDefault();
        void store.save();
      }}
    >
      <ScrollArea className="flex min-h-0 flex-1 flex-col gap-3.5">
        <section className="flex flex-none flex-col gap-3.25 rounded-card border border-main-700 bg-main-900 p-4">
          <div className="flex items-center gap-2">
            <h2 className="flex-1 text-[11px] font-semibold tracking-[0.08em] text-main-400 uppercase">
              {vm.isNew ? "Новое хранилище" : "Настройки хранилища"}
            </h2>
            <Chip>LanceDB</Chip>
          </div>
          <Field label="Название" htmlFor="vs-name" required error={vm.errors.name}>
            <TextInput
              id="vs-name"
              value={vm.name}
              placeholder="База знаний продукта"
              maxLength={128}
              invalid={!!vm.errors.name}
              disabled={store.busy}
              onChange={(e) => vm.set("name", e.target.value)}
            />
          </Field>
          <Field
            label="Описание"
            htmlFor="vs-description"
            optionalHint
            error={vm.errors.description}
          >
            <TextArea
              id="vs-description"
              value={vm.description}
              placeholder="Какие документы хранятся здесь и для чего используются"
              maxLength={4096}
              invalid={!!vm.errors.description}
              disabled={store.busy}
              onChange={(e) => vm.set("description", e.target.value)}
            />
          </Field>
        </section>
        <section className="flex flex-none flex-col gap-3.25 rounded-card border border-main-700 bg-main-900 p-4">
          <h2 className="text-[11px] font-semibold tracking-[0.08em] text-main-400 uppercase">
            Эмбеддинги и индексация
          </h2>
          <div className="grid grid-cols-1 gap-3.25 lg:grid-cols-2">
            <Field label="Embedding-провайдер" required error={vm.errors.embeddingProviderId}>
              <SelectInput
                label="Embedding-провайдер"
                placeholder="Выберите провайдер"
                value={vm.embeddingProviderId}
                invalid={!!vm.errors.embeddingProviderId}
                disabled={!vm.isNew || store.busy}
                options={vm.providers
                  .filter((p) => p.enabled || p.id === vm.embeddingProviderId)
                  .map((p) => ({ value: p.id, label: p.name }))}
                onChange={(value) => vm.set("embeddingProviderId", value)}
              />
            </Field>
            <Field
              label="Модель эмбеддингов"
              htmlFor="vs-embeddingModelId"
              required
              error={vm.errors.embeddingModelId}
            >
              <TextInput
                id="vs-embeddingModelId"
                value={vm.embeddingModelId}
                placeholder="mxbai-embed-large"
                mono
                invalid={!!vm.errors.embeddingModelId}
                disabled={!vm.isNew || store.busy}
                onChange={(e) => vm.set("embeddingModelId", e.target.value)}
              />
            </Field>
            {fields.map(([key, label]) => (
              <Field
                key={key}
                label={label}
                htmlFor={`vs-${key}`}
                required
                {...(vm.errors[key] ? { error: vm.errors[key] } : {})}
              >
                <TextInput
                  id={`vs-${key}`}
                  value={vm[key]}
                  mono
                  inputMode="numeric"
                  invalid={!!vm.errors[key]}
                  placeholder={key === "dimension" ? "Например, 1024" : undefined}
                  disabled={
                    store.busy ||
                    (!vm.isNew && key === "dimension") ||
                    (vm.chunkLocked && (key === "chunkSize" || key === "chunkOverlap"))
                  }
                  onChange={(e) => vm.set(key, e.target.value)}
                />
              </Field>
            ))}
            <Field label="Метрика">
              <SelectInput
                label="Метрика"
                value={vm.metric}
                options={["cosine", "l2", "dot"].map((value) => ({ value, label: value }))}
                disabled={!vm.isNew || store.busy}
                onChange={vm.setMetric}
              />
            </Field>
          </div>
          <p className="text-[11px] leading-relaxed text-main-500">
            LanceDB · FLAT. Провайдер, модель, размерность и метрика фиксируются при создании.
            Размер чанка и перекрытие можно менять до индексации.
          </p>
          {vm.isNew && !vm.providers.some((p) => p.enabled) ? (
            <p
              role="status"
              className="rounded-[6px] border border-dashed border-main-600 px-3 py-2.5 text-xs text-warn"
            >
              Сначала добавьте включённый embedding-провайдер на странице «AI-провайдеры».
            </p>
          ) : null}
        </section>
        <div className="flex flex-none items-center gap-3 rounded-card border border-main-700 bg-main-900 px-4 py-3">
          <Button type="submit" tone="primary" disabled={store.busy}>
            {store.busy ? "Сохранение…" : "Сохранить"}
          </Button>
          <Button
            type="button"
            tone="secondary"
            disabled={store.busy}
            needConfirm
            modalSetup={{
              title: "Закрыть форму?",
              content: "Несохранённые изменения будут потеряны.",
              confirmLabel: "Закрыть",
            }}
            onClick={store.cancel}
          >
            Отмена
          </Button>
        </div>
      </ScrollArea>
    </form>
  );
}
export default observer(VectorStoreForm);
