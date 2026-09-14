import { observer } from "mobx-react-lite";
import type { VectorOcrLanguage } from "@zvs/shared";
import SelectInput from "../../ui/atoms/SelectInput";
import TextInput from "../../ui/atoms/TextInput";
import Toggle from "../../ui/atoms/Toggle";
import Field from "../../ui/molecules/Field";
import type VectorStoreFormVm from "./VectorStoreFormVm";

const LANGUAGES: { value: VectorOcrLanguage; label: string }[] = [
  { value: "auto", label: "Определять автоматически" },
  { value: "rus", label: "Русский" },
  { value: "eng", label: "Английский" },
  { value: "rus+eng", label: "Русский и английский" },
];

/**
 * Reranking and OCR are optional stages, so each is a section that stays collapsed to a
 * single switch until it is turned on. Both offer only what the Downloads page has already
 * installed — a stage pointing at a model that is not on disk would fail at index time.
 */
function VectorStoreAdvanced({
  vm,
  disabled,
}: {
  readonly vm: VectorStoreFormVm;
  readonly disabled: boolean;
}) {
  return (
    <div className="flex flex-col gap-3.25">
      <div className="flex flex-col gap-3.25 rounded-[9px] border border-main-750 bg-main-800 p-3.5">
        <Toggle
          id="vs-rerank"
          checked={vm.rerankEnabled}
          disabled={disabled || (vm.rerankModels.length === 0 && !vm.rerankEnabled)}
          label="Переранжирование результатов"
          onChange={vm.setRerankEnabled}
        />
        <p className="text-[11px] leading-relaxed text-main-500">
          Кросс-энкодер заново упорядочивает найденные фрагменты: поиск отдаёт больше кандидатов,
          модель оставляет сверху действительно подходящие.
        </p>
        {vm.rerankModels.length === 0 ? (
          <p className="text-[11px] text-warn">
            Нет установленных моделей переранжирования. Скачайте, например, BGE Reranker v2 M3 на
            странице «Загрузки».
          </p>
        ) : null}
        {vm.rerankEnabled ? (
          <div className="grid grid-cols-1 gap-3.25 lg:grid-cols-2">
            <Field label="Модель переранжирования" required error={vm.errors.rerank}>
              <SelectInput
                label="Модель переранжирования"
                placeholder="Выберите установленную модель"
                value={vm.rerankModelRef}
                invalid={!!vm.errors.rerank}
                disabled={disabled}
                options={vm.rerankModels.map((item) => ({
                  value: item.ref,
                  label: item.displayName,
                }))}
                onChange={(value) => vm.set("rerankModelRef", value)}
              />
            </Field>
            <Field
              label="Кандидатов до переранжирования"
              htmlFor="vs-rerank-candidates"
              help="Сколько фрагментов поиск отдаёт модели: от 1 до 500."
            >
              <TextInput
                id="vs-rerank-candidates"
                value={vm.rerankCandidatesCount}
                mono
                inputMode="numeric"
                disabled={disabled}
                onChange={(e) => vm.set("rerankCandidatesCount", e.target.value)}
              />
            </Field>
          </div>
        ) : null}
      </div>
      <div className="flex flex-col gap-3.25 rounded-[9px] border border-main-750 bg-main-800 p-3.5">
        <Toggle
          id="vs-ocr"
          checked={vm.ocrEnabled}
          disabled={disabled || (vm.ocrModels.length === 0 && !vm.ocrEnabled)}
          label="OCR для сканов и изображений"
          onChange={vm.setOcrEnabled}
        />
        <p className="text-[11px] leading-relaxed text-main-500">
          Страницы, из которых почти не удалось извлечь текст, распознаёт зрительно-языковая модель
          — локально, без обращения к сети.
        </p>
        {vm.ocrModels.length === 0 ? (
          <p className="text-[11px] text-warn">
            Нет установленных моделей OCR. Скачайте, например, Qwen2.5-VL 7B на странице «Загрузки».
          </p>
        ) : null}
        {vm.ocrEnabled ? (
          <div className="grid grid-cols-1 gap-3.25 lg:grid-cols-2">
            <Field label="Модель OCR" required error={vm.errors.ocr}>
              <SelectInput
                label="Модель OCR"
                placeholder="Выберите установленную модель"
                value={vm.ocrModelRef}
                invalid={!!vm.errors.ocr}
                disabled={disabled}
                options={vm.ocrModels.map((item) => ({ value: item.ref, label: item.displayName }))}
                onChange={(value) => vm.set("ocrModelRef", value)}
              />
            </Field>
            <Field label="Язык распознавания">
              <SelectInput
                label="Язык распознавания"
                value={vm.ocrLanguage}
                disabled={disabled}
                options={LANGUAGES}
                onChange={vm.setOcrLanguage}
              />
            </Field>
            <Field
              label="Порог распознавания, символов на страницу"
              htmlFor="vs-ocr-threshold"
              help="Ниже этого числа извлечённого текста страница считается сканом."
            >
              <TextInput
                id="vs-ocr-threshold"
                value={vm.ocrMinCharsPerPage}
                mono
                inputMode="numeric"
                disabled={disabled}
                onChange={(e) => vm.set("ocrMinCharsPerPage", e.target.value)}
              />
            </Field>
          </div>
        ) : null}
      </div>
    </div>
  );
}
export default observer(VectorStoreAdvanced);
