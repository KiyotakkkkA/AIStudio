import { mdiAutoFix, mdiChevronDown, mdiChevronRight } from "@mdi/js";
import { observer } from "mobx-react-lite";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { useStore } from "../../stores/useStore";
import Button from "../../ui/atoms/Button";
import Icon from "../../ui/atoms/Icon";
import SelectInput from "../../ui/atoms/SelectInput";
import TextInput from "../../ui/atoms/TextInput";
import Field from "../../ui/molecules/Field";
import TextArea from "../../ui/atoms/TextArea";
import Chip from "../../ui/atoms/Chip";
import VectorStoreAdvanced from "./VectorStoreAdvanced";

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
        <section className="flex flex-none flex-col gap-3.25 rounded-card border border-main-750 bg-main-900 p-4">
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
        <section className="flex flex-none flex-col gap-3.25 rounded-card border border-main-750 bg-main-900 p-4">
          <div className="flex flex-wrap items-center gap-2">
            <h2 className="flex-1 text-[11px] font-semibold tracking-[0.08em] text-main-400 uppercase">
              Эмбеддинги и индексация
            </h2>
            <Button
              type="button"
              tone="ghost"
              disabled={store.busy}
              title="Подобрать все настройки, кроме названия и описания, по этому устройству и установленным моделям"
              onClick={vm.autofill}
            >
              <Icon path={mdiAutoFix} size={15} />
              Заполнить автоматически
            </Button>
          </div>
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
            <Field label="Модель эмбеддингов" required error={vm.errors.embeddingModelId}>
              <SelectInput
                label="Модель эмбеддингов"
                value={vm.embeddingModelId}
                placeholder="Выберите модель"
                invalid={!!vm.errors.embeddingModelId}
                disabled={!vm.isNew || store.busy || !vm.embeddingProviderId}
                options={vm.embeddingModels.map((model) => ({
                  value: model.externalId,
                  label: model.displayName || model.externalId,
                }))}
                onChange={(value) => vm.set("embeddingModelId", value)}
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
          {vm.autofillNote === "" ? null : (
            <p role="status" className="text-[11px] leading-relaxed text-accent-medium">
              {vm.autofillNote}
            </p>
          )}
          <p className="text-[11px] leading-relaxed text-main-500">
            LanceDB · FLAT. Провайдер, модель, размерность и метрика фиксируются при создании.
            Размер чанка и перекрытие можно менять до индексации.
          </p>
          {vm.isNew && !vm.providers.some((p) => p.enabled) ? (
            <p
              role="status"
              className="rounded-[6px] border border-dashed border-main-750 px-3 py-2.5 text-xs text-warn"
            >
              Сначала добавьте включённый embedding-провайдер на странице «AI-провайдеры».
            </p>
          ) : null}
        </section>
        <section className="flex flex-none flex-col gap-3.25 rounded-card border border-main-750 bg-main-900 p-4">
          <button
            type="button"
            aria-expanded={vm.advancedOpen}
            className="flex items-center gap-2 text-left"
            onClick={vm.toggleAdvanced}
          >
            <Icon
              path={vm.advancedOpen ? mdiChevronDown : mdiChevronRight}
              size={16}
              className="text-main-400"
            />
            <h2 className="flex-1 text-[11px] font-semibold tracking-[0.08em] text-main-400 uppercase">
              Расширенные настройки
            </h2>
            {vm.rerankEnabled ? <Chip>Переранжирование</Chip> : null}
            {vm.ocrEnabled ? <Chip>OCR</Chip> : null}
          </button>
          {vm.advancedOpen ? <VectorStoreAdvanced vm={vm} disabled={store.busy} /> : null}
        </section>
        <div className="flex flex-none items-center gap-3 rounded-card border border-main-750 bg-main-900 px-4 py-3">
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
