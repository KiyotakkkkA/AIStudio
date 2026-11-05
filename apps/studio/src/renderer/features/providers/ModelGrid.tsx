import { mdiCubeOutline } from "@mdi/js";
import { InputCheckBox } from "@kiyotakkkka/zvs-uikit-lib";
import { observer } from "mobx-react-lite";
import { useState } from "react";
import Chip from "../../ui/atoms/Chip";
import TextInput from "../../ui/atoms/TextInput";
import Icon from "../../ui/atoms/Icon";
import { useStore } from "../../stores/useStore";
import SelectInput from "../../ui/atoms/SelectInput";
import ModelTable from "./ModelTable";
import {
  modelTableRows,
  type ModelTableFilter,
  type ModelTableSort,
  type ModelSortDirection,
} from "./modelTableModel";

function ModelGrid() {
  const { providers } = useStore();
  const [filter, setFilter] = useState<ModelTableFilter>("all");
  const [sort, setSort] = useState<ModelTableSort>("name");
  const [direction, setDirection] = useState<ModelSortDirection>("ascending");
  const selectedIds = providers.form?.selectedModelIds ?? [];
  const rows = modelTableRows(providers.visibleRows, selectedIds, filter, sort, direction);
  const outcome = providers.probeResult?.outcome;
  const curated = outcome?.kind === "ok" && !outcome.live;

  return (
    <section className="mx-auto flex min-h-0 w-full max-w-360 flex-none flex-col gap-4 rounded-xl bg-main-850 p-5">
      <div className="flex flex-none flex-wrap items-center gap-3">
        <h2 className="flex min-w-48 flex-1 flex-col gap-1 text-[15px] font-semibold tracking-[-0.01em] text-main-100">
          Доступные модели
          <span className="text-[12px] font-normal tracking-normal text-main-500">
            {providers.rows.length === 0
              ? "Список моделей появится после проверки связи"
              : curated
                ? "Сохранённый список моделей"
                : "Список моделей получен"}
          </span>
        </h2>
        {providers.rows.length > 0 ? (
          <span className="flex items-center gap-3 text-[12px] text-main-400">
            <span>Выбрано: {selectedIds.length.toLocaleString("ru-RU")}</span>
            <Chip>{`${rows.length.toLocaleString("ru-RU")} из ${providers.rows.length.toLocaleString("ru-RU")}`}</Chip>
          </span>
        ) : null}
      </div>

      {providers.rows.length > 0 ? (
        <div className="flex min-w-0 flex-none items-center gap-3 overflow-x-auto pb-1 text-[12px] whitespace-nowrap text-main-300">
          <span className="w-52 flex-none">
            <TextInput
              preset="search"
              aria-label="Фильтр моделей"
              value={providers.modelQuery}
              placeholder="Фильтр моделей…"
              onChange={(event) => providers.setModelQuery(event.target.value)}
            />
          </span>
          <span className="w-44 flex-none">
            <SelectInput
              label="Показать модели"
              value={filter}
              onChange={(value) => setFilter(value as ModelTableFilter)}
              options={[
                { value: "all", label: "Все модели" },
                { value: "selected", label: "Только выбранные" },
                { value: "available", label: "Доступные" },
                { value: "unavailable", label: "Недоступные" },
              ]}
            />
          </span>
          <span className="w-52 flex-none">
            <SelectInput
              label="Сортировка моделей"
              value={`${sort}:${direction}`}
              onChange={(value) => {
                const [column, order] = value.split(":") as [ModelTableSort, ModelSortDirection];
                setSort(column);
                setDirection(order);
              }}
              options={[
                { value: "name:ascending", label: "Название: А → Я" },
                { value: "name:descending", label: "Название: Я → А" },
                { value: "size:ascending", label: "Размер: по возрастанию" },
                { value: "size:descending", label: "Размер: по убыванию" },
                { value: "context:ascending", label: "Контекст: по возрастанию" },
                { value: "context:descending", label: "Контекст: по убыванию" },
                { value: "availability:descending", label: "Сначала доступные" },
                { value: "availability:ascending", label: "Сначала недоступные" },
              ]}
            />
          </span>
          {providers.supportsFreeFilter ? (
            <InputCheckBox
              checked={providers.freeModelsOnly}
              onChange={providers.setFreeModelsOnly}
            >
              Бесплатные модели
            </InputCheckBox>
          ) : null}
          {providers.modelProviderKind === "openrouter" ? (
            <InputCheckBox
              checked={providers.noTrainingOnly}
              onChange={providers.setNoTrainingOnly}
            >
              Не используют данные для обучения
            </InputCheckBox>
          ) : null}
        </div>
      ) : null}

      {providers.rows.length === 0 ? (
        <div className="flex items-center gap-3 py-2 text-main-400">
          <Icon path={mdiCubeOutline} size={22} className="flex-none text-main-500" />
          <p className="text-[12px]">
            Нажмите «Проверить связь», чтобы загрузить модели провайдера.
          </p>
        </div>
      ) : rows.length === 0 ? (
        <p className="rounded-card bg-main-800 px-3 py-4.5 text-center text-[12px] text-main-400">
          Ничего не найдено по фильтру.
        </p>
      ) : (
        <ModelTable
          rows={rows}
          selectedIds={selectedIds}
          selectable={providers.form !== null && !providers.saving}
          sort={sort}
          direction={direction}
          onSort={(column) => {
            if (column === sort)
              setDirection(direction === "ascending" ? "descending" : "ascending");
            else {
              setSort(column);
              setDirection("ascending");
            }
          }}
          onToggle={(externalId) => providers.form?.toggleModel(externalId)}
        />
      )}
    </section>
  );
}

export default observer(ModelGrid);
