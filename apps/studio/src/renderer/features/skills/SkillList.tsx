import { mdiBookOpenOutline, mdiPlus } from "@mdi/js";
import { observer } from "mobx-react-lite";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import StatusDot from "../../ui/atoms/StatusDot";
import TextInput from "../../ui/atoms/TextInput";
import EmptyState from "../../ui/molecules/EmptyState";
import ListRow from "../../ui/molecules/ListRow";
import ListRowSkeleton from "../../ui/molecules/ListRowSkeleton";
import type SkillStore from "./SkillStore";
import { skillLine, skillTone, SOURCE_LABELS } from "./skillPresentation";

function SkillList({ store }: { readonly store: SkillStore }) {
  const draft = store.form?.isNew === true ? store.form : null;

  return (
    <div className="flex min-h-0 w-65.5 flex-none flex-col gap-2 border-r border-line py-3.5 pl-3 pr-2.5">
      <div className="flex-none px-0.5 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
        Навыки
      </div>
      <TextInput
        preset="search"
        aria-label="Поиск навыков"
        value={store.query}
        placeholder="Поиск навыков…"
        onChange={(event) => {
          store.setQuery(event.target.value);
        }}
      />

      <ScrollArea className="flex min-h-0 flex-1 flex-col gap-2 pr-0.5">
        {store.loading && store.skills.length === 0 ? (
          <>
            <ListRowSkeleton />
            <ListRowSkeleton />
          </>
        ) : null}

        {store.isEmpty && draft === null ? (
          <div className="flex items-center justify-center rounded-card bg-main-800 px-2">
            <EmptyState
              icon={mdiBookOpenOutline}
              title="Навыков нет"
              description="Создайте навык или импортируйте папку с файлом SKILL.md."
            />
          </div>
        ) : null}

        {store.visibleSkills.map((skill) => {
          const invalid = skill.status === "invalid";
          return (
            <div key={skill.id} className={skill.enabled ? undefined : "opacity-55"}>
              <ListRow
                selected={store.selectedId === skill.id && draft === null}
                onClick={() => {
                  store.requestSelect(skill.id);
                }}
              >
                <span className="flex min-w-0 items-center gap-2">
                  <StatusDot tone={skillTone(skill)} />
                  <span className="flex-1 truncate text-[12.5px] font-medium text-main-50">
                    {skill.name}
                  </span>
                  <Chip>{SOURCE_LABELS[skill.source]}</Chip>
                </span>
                <span className={`truncate text-[11px] ${invalid ? "text-err" : "text-main-400"}`}>
                  <span className="font-mono">{skill.key}</span> · {skillLine(skill)}
                </span>
              </ListRow>
            </div>
          );
        })}

        {draft === null ? null : (
          <ListRow selected>
            <span className="flex min-w-0 items-center gap-2">
              <StatusDot tone="idle" />
              <span className="flex-1 truncate text-[12.5px] font-medium text-main-50">
                {draft.name.trim().length > 0 ? draft.name : "Новый навык"}
              </span>
            </span>
            <span className="truncate text-[11px] text-main-400">Не сохранён</span>
          </ListRow>
        )}

        <ListRow
          dashed
          onClick={store.requestCreate}
          leading={
            <span className="flex size-7.5 flex-none items-center justify-center rounded-control bg-main-700 text-main-400">
              <Icon path={mdiPlus} size={14} />
            </span>
          }
        >
          <span className="font-medium text-main-300">Создать навык</span>
          <span className="truncate text-[11px] text-main-400">Инструкция, описание и файлы</span>
        </ListRow>
      </ScrollArea>
    </div>
  );
}

export default observer(SkillList);
