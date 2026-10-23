import { mdiArrowLeft, mdiDeleteOutline, mdiGraphOutline, mdiPlus } from "@mdi/js";
import { observer } from "mobx-react-lite";
import { useEffect, useState, type ReactNode } from "react";
import type { ScenarioDto } from "@zvs/shared";
import Button from "../../ui/atoms/buttons/Button";
import IconButton from "../../ui/atoms/buttons/IconButton";
import Icon from "../../ui/atoms/Icon";
import StatusDot from "../../ui/atoms/StatusDot";
import Toggle from "../../ui/atoms/Toggle";
import EmptyState from "../../ui/molecules/EmptyState";
import PageHeader from "../../ui/organisms/PageHeader";
import { useStore } from "../../stores/useStore";
import { formatLastUsed } from "../tools/toolPresentation";
import NodeInspector from "./NodeInspector";
import ScenarioCanvas from "./ScenarioCanvas";
import type ScenarioEditor from "./ScenarioEditor";
import type ScenarioStore from "./ScenarioStore";
import { issuesWord, savedAtLabel, scenariosWord } from "./scenarioCopy";

function Banner({
  tone,
  children,
  action,
}: {
  readonly tone: "err" | "ok";
  readonly children: string;
  readonly action: ReactNode;
}) {
  return (
    <div
      role={tone === "err" ? "alert" : "status"}
      className={`mx-4.5 mt-3 flex flex-none items-center gap-3 rounded-control px-3 py-2.25 text-[12px] ${
        tone === "err" ? "bg-err-soft text-err" : "bg-ok-soft text-ok"
      }`}
    >
      <span className="flex-1">{children}</span>
      {action}
    </div>
  );
}

function draftLine(editor: ScenarioEditor): string {
  if (!editor.dirty) return "Изменений нет";
  if (editor.draftState === "failed") return "Черновик не сохранён";
  if (editor.draftState === "saved" && editor.draftSavedAt !== null) {
    return `Черновик сохранён в ${savedAtLabel(editor.draftSavedAt)}`;
  }
  return "Сохранение черновика…";
}

const EditorHeader = observer(function EditorHeader({
  store,
  editor,
}: {
  readonly store: ScenarioStore;
  readonly editor: ScenarioEditor;
}) {
  const scenario = editor.scenario;
  const [name, setName] = useState(scenario?.name ?? "");
  useEffect(() => {
    setName(scenario?.name ?? "");
  }, [scenario?.name]);

  return (
    <header className="flex h-14 flex-none items-center gap-3 border-b border-line bg-main-900 px-4.5">
      <IconButton
        path={mdiArrowLeft}
        label="К списку сценариев"
        tooltip="bottom-left"
        iconSize={18}
        className="size-8"
        onClick={store.close}
      />
      <input
        aria-label="Название сценария"
        value={name}
        maxLength={120}
        className="min-w-0 max-w-90 flex-initial rounded-control border border-transparent bg-transparent px-1.5 py-1 text-[14.5px] font-semibold text-main-50 outline-none hover:border-line focus:border-accent-dark"
        onChange={(event) => {
          setName(event.target.value);
        }}
        onBlur={() => {
          if (name.trim().length === 0) setName(scenario?.name ?? "");
          else void store.rename(name);
        }}
        onKeyDown={(event) => {
          if (event.key === "Enter") event.currentTarget.blur();
        }}
      />
      {scenario?.tags.map((tag) => (
        <span
          key={tag}
          className="inline-flex h-5 items-center rounded-[5px] bg-main-700 px-1.75 text-[10.5px] font-medium text-main-300"
        >
          {tag}
        </span>
      ))}
      {scenario === null ? null : (
        <span
          className="inline-flex h-5 items-center rounded-[5px] bg-main-700 px-1.75 font-mono text-[10.5px] font-medium text-main-300"
          title="Текущая версия"
        >
          {`v${String(scenario.version)}`}
        </span>
      )}
      <span className="flex-1" />
      <span className="text-[11.5px] text-main-500">{draftLine(editor)}</span>
      {scenario === null ? null : (
        <Toggle
          label="Активен"
          checked={scenario.active}
          disabled={editor.switching}
          onChange={(active) => void editor.setActive(active)}
        />
      )}
      <Button
        type="button"
        tone="primary"
        disabled={!editor.dirty || editor.saving}
        onClick={() => void editor.save()}
      >
        Сохранить
      </Button>
    </header>
  );
});

const EditorView = observer(function EditorView({
  store,
  editor,
}: {
  readonly store: ScenarioStore;
  readonly editor: ScenarioEditor;
}) {
  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col">
      <EditorHeader store={store} editor={editor} />
      {editor.error === null ? null : (
        <Banner
          tone="err"
          action={
            <button type="button" className="text-main-400" onClick={editor.dismissError}>
              Скрыть
            </button>
          }
        >
          {editor.error}
        </Banner>
      )}
      {editor.notice === null ? null : (
        <Banner
          tone="ok"
          action={
            <>
              <button
                type="button"
                className="text-main-300 hover:text-main-100"
                onClick={() => void editor.discardDraft()}
              >
                Отбросить черновик
              </button>
              <button type="button" className="text-main-400" onClick={editor.dismissNotice}>
                Скрыть
              </button>
            </>
          }
        >
          {editor.notice}
        </Banner>
      )}
      {!editor.loaded ? (
        <div className="flex flex-1 items-center justify-center text-[12.5px] text-main-500">
          Загрузка сценария…
        </div>
      ) : editor.scenario === null ? null : (
        <div className="mt-3 flex min-h-0 flex-1 border-t border-line">
          <ScenarioCanvas editor={editor} />
          <NodeInspector editor={editor} />
        </div>
      )}
    </div>
  );
});

function scenarioLine(scenario: ScenarioDto, now: number): string {
  const parts = [
    `v${String(scenario.version)}`,
    `изменён ${formatLastUsed(scenario.updatedAt, now)}`,
  ];
  if (scenario.issueCount > 0) parts.push(issuesWord(scenario.issueCount));
  return parts.join(" · ");
}

const ScenarioList = observer(function ScenarioList({ store }: { readonly store: ScenarioStore }) {
  const now = Date.now();
  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col">
      <PageHeader
        icon={mdiGraphOutline}
        title="Сценарии"
        subtitle={store.loaded ? scenariosWord(store.scenarios.length) : "Загрузка…"}
        actions={
          <Button
            type="button"
            tone="primary"
            disabled={store.creating}
            onClick={() => void store.create()}
          >
            <Icon path={mdiPlus} size={15} />
            Новый сценарий
          </Button>
        }
      />
      {store.error === null ? null : (
        <Banner
          tone="err"
          action={
            <button type="button" className="text-main-400" onClick={store.dismissError}>
              Скрыть
            </button>
          }
        >
          {store.error}
        </Banner>
      )}
      {store.isEmpty ? (
        <div className="flex flex-1 items-center justify-center">
          <EmptyState
            icon={mdiGraphOutline}
            title="Сценариев нет"
            description="Сценарий связывает триггер, ИИ-агента и действия в граф, который запускается сам."
            action={
              <Button type="button" tone="primary" onClick={() => void store.create()}>
                <Icon path={mdiPlus} size={15} />
                Новый сценарий
              </Button>
            }
          />
        </div>
      ) : (
        <ul className="m-0 flex min-h-0 flex-1 list-none flex-col gap-2 overflow-y-auto px-6 pt-1 pb-6">
          {store.scenarios.map((scenario) => (
            <li
              key={scenario.id}
              className="flex items-center gap-3 rounded-card border border-line bg-main-850 px-4 py-3 shadow-card hover:border-line-strong"
            >
              <StatusDot
                tone={scenario.active ? "ok" : scenario.issueCount > 0 ? "err" : "idle"}
                title={scenario.active ? "Активен" : "Не активен"}
              />
              <button
                type="button"
                className="flex min-w-0 flex-1 flex-col text-left"
                onClick={() => void store.open(scenario.id)}
              >
                <span className="truncate text-[13.5px] font-semibold text-main-50">
                  {scenario.name}
                </span>
                <span className="truncate text-[11.5px] text-main-500">
                  {scenarioLine(scenario, now)}
                </span>
              </button>
              {scenario.tags.map((tag) => (
                <span
                  key={tag}
                  className="inline-flex h-5 flex-none items-center rounded-[5px] bg-main-700 px-1.75 text-[10.5px] font-medium text-main-300"
                >
                  {tag}
                </span>
              ))}
              <Button
                type="button"
                tone="ghost"
                aria-label="Удалить сценарий"
                disabled={store.removingId === scenario.id}
                needConfirm
                modalSetup={{
                  title: "Удалить сценарий?",
                  content: (
                    <>
                      Сценарий «{scenario.name}» и все его версии будут удалены. Действие нельзя
                      отменить.
                    </>
                  ),
                  tone: "danger",
                  confirmLabel: "Удалить",
                }}
                onClick={() => void store.remove(scenario.id)}
              >
                <Icon path={mdiDeleteOutline} size={15} />
              </Button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
});

function ScenariosWorkspace() {
  const { scenarios: store } = useStore();

  useEffect(() => {
    void store.load();
    return () => {
      store.close();
    };
  }, [store]);

  return store.editor === null ? (
    <ScenarioList store={store} />
  ) : (
    <EditorView store={store} editor={store.editor} />
  );
}

export default observer(ScenariosWorkspace);
