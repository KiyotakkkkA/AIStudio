import { mdiAutoFix, mdiChevronDown, mdiChevronRight, mdiInformationOutline } from "@mdi/js";
import { observer } from "mobx-react-lite";
import { Floating, ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { useStore } from "../../stores/useStore";
import Button from "../../ui/atoms/Button";
import Icon from "../../ui/atoms/Icon";
import SelectInput from "../../ui/atoms/SelectInput";
import TextInput from "../../ui/atoms/TextInput";
import Field from "../../ui/molecules/Field";
import TextArea from "../../ui/atoms/TextArea";
import Chip from "../../ui/atoms/Chip";
import VectorStoreAdvanced from "./VectorStoreAdvanced";
import { InfoButton } from "../../ui/atoms/InfoButton";

const METRIC_OPTIONS = [
  { value: "cosine", label: "Косинусное сходство" },
  { value: "l2", label: "Евклидово расстояние" },
  { value: "dot", label: "Скалярное произведение" },
];

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
                <div className="flex items-center gap-2">
                  <div className="min-w-0 flex-1">
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
                  </div>
                  <InfoButton label={`Что означает поле «${label}»`}>
                    {key === "dimension" ? (
                      <>
                        <p className="mb-2 font-medium text-main-50">Размерность</p>
                        <p>
                          Количество чисел в описании каждого фрагмента текста. Обычно её задаёт
                          выбранная embedding-модель, поэтому значение должно совпадать с моделью.
                        </p>
                      </>
                    ) : key === "chunkSize" ? (
                      <>
                        <p className="mb-2 font-medium text-main-50">Размер чанка</p>
                        <p>
                          Сколько частей текста помещается в один фрагмент для поиска. Большие
                          значения сохраняют больше контекста, маленькие точнее находят отдельные
                          места.
                        </p>
                      </>
                    ) : (
                      <>
                        <p className="mb-2 font-medium text-main-50">Перекрытие</p>
                        <p>
                          Сколько частей текста повторяется между соседними фрагментами. Это
                          помогает не потерять смысл на границе, но увеличивает объём индекса.
                        </p>
                      </>
                    )}
                  </InfoButton>
                </div>
              </Field>
            ))}
            <Field label="Метрика">
              <div className="flex items-center gap-2">
                <div className="min-w-0 flex-1">
                  <SelectInput
                    label="Метрика"
                    value={vm.metric}
                    options={METRIC_OPTIONS}
                    disabled={!vm.isNew || store.busy}
                    onChange={vm.setMetric}
                  />
                </div>
                <Floating anchor="bottom-right">
                  <Floating.Trigger>
                    <button
                      type="button"
                      aria-label="Что означают метрики"
                      className="flex size-7 items-center justify-center rounded-full text-main-400 transition-colors hover:bg-main-750 hover:text-main-100 focus-visible:outline-2 focus-visible:outline-accent-dark"
                    >
                      <Icon path={mdiInformationOutline} size={16} />
                    </button>
                  </Floating.Trigger>
                  <Floating.Content className="w-72 border border-main-700 bg-main-850 p-3 text-[11px] leading-relaxed text-main-200 shadow-lg">
                    <p className="mb-2 font-medium text-main-50">Как сравниваются фрагменты</p>
                    <p>
                      <strong>Косинусное сходство</strong> сравнивает направление смыслов. Обычно
                      это лучший универсальный вариант для поиска по тексту.
                    </p>
                    <p className="mt-2">
                      <strong>Евклидово расстояние</strong> сравнивает расстояние между векторами:
                      чем меньше значение, тем ближе фрагменты.
                    </p>
                    <p className="mt-2">
                      <strong>Скалярное произведение</strong> оценивает совпадение направлений и
                      длины векторов. Используйте его, если это рекомендует модель.
                    </p>
                  </Floating.Content>
                </Floating>
              </div>
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
