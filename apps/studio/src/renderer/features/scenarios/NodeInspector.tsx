import {
  mdiCancel,
  mdiChevronDown,
  mdiChevronRight,
  mdiClose,
  mdiContentCopy,
  mdiDeleteOutline,
  mdiLinkVariantOff,
  mdiPlay,
  mdiPlayOutline,
} from "@mdi/js";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { observer } from "mobx-react-lite";
import { useState } from "react";
import type {
  Json,
  ScenarioNodeDto,
  ScenarioNodeSettingsDto,
  ScenarioOnError,
  ScenarioOverlap,
  ScenarioParams,
} from "@zvs/shared";
import Button from "../../ui/atoms/buttons/Button";
import IconButton from "../../ui/atoms/buttons/IconButton";
import Icon from "../../ui/atoms/Icon";
import SelectInput from "../../ui/atoms/SelectInput";
import StatusDot from "../../ui/atoms/StatusDot";
import TextArea from "../../ui/atoms/TextArea";
import TextInput from "../../ui/atoms/TextInput";
import Toggle from "../../ui/atoms/Toggle";
import Field from "../../ui/molecules/Field";
import {
  ENTITY_PLACEHOLDERS,
  entityKindOf,
  MODEL_NEEDS_PROVIDER,
  withCurrent,
  withParam,
  type EntityKind,
} from "./entityParams";
import { attachmentLinkId, childrenIn, findNode } from "./graphModel";
import { iconFor } from "./flowModel";
import { descriptionOf, formFields, withField, type FormField } from "./nodeForms";
import type ScenarioEditor from "./ScenarioEditor";
import {
  issueText,
  ROLE_LABELS,
  slotLabel,
  stepsWord,
  TOOL_SET_EMPTY,
  toolCountLabel,
} from "./scenarioCopy";
import {
  ON_ERROR_LABELS,
  OVERLAP_HINT,
  OVERLAP_LABELS,
  PINNED_INVALID,
  STEP_STATUS_LABELS,
} from "./runCopy";
import { TOOL_SET_PARAM, TOOL_SET_TYPE, toolSetKeys } from "./toolSet";
import ToolSetPicker from "./ToolSetPicker";

const PARAMS_INVALID = "Параметры должны быть JSON-объектом";
const SECTION = "m-0 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase";

type InspectorTab = "params" | "settings" | "notes";

const TAB_LABELS: Record<InspectorTab, string> = {
  params: "Параметры",
  settings: "Настройки",
  notes: "Заметки",
};

const ON_ERROR_OPTIONS = (Object.keys(ON_ERROR_LABELS) as ScenarioOnError[]).map((value) => ({
  value,
  label: ON_ERROR_LABELS[value],
}));

const OVERLAP_OPTIONS = (Object.keys(OVERLAP_LABELS) as ScenarioOverlap[]).map((value) => ({
  value,
  label: OVERLAP_LABELS[value],
}));

function paramText(value: Json | undefined): string {
  if (value === undefined || value === null) return "";
  return typeof value === "string" ? value : JSON.stringify(value);
}

function parseParams(text: string): ScenarioParams | null {
  try {
    const value: unknown = JSON.parse(text.trim().length === 0 ? "{}" : text);
    if (value === null || typeof value !== "object" || Array.isArray(value)) return null;
    return value as ScenarioParams;
  } catch {
    return null;
  }
}

function parseJson(text: string): { readonly value: Json | null } | null {
  if (text.trim().length === 0) return { value: null };
  try {
    return { value: JSON.parse(text) as Json };
  } catch {
    return null;
  }
}

function wholeNumber(text: string): number | undefined {
  const value = Number(text.trim().replace(",", "."));
  return text.trim().length === 0 || !Number.isFinite(value) ? undefined : Math.round(value);
}

function CommitInput({
  id,
  value,
  placeholder,
  mono = false,
  multiline = false,
  invalid = false,
  onCommit,
}: {
  readonly id: string;
  readonly value: string;
  readonly placeholder?: string;
  readonly mono?: boolean;
  readonly multiline?: boolean;
  readonly invalid?: boolean;
  readonly onCommit: (value: string) => void;
}) {
  const [draft, setDraft] = useState<string | null>(null);
  const events = {
    onFocus: () => {
      setDraft(value);
    },
    onBlur: () => {
      if (draft !== null && draft !== value) onCommit(draft);
      setDraft(null);
    },
  };
  if (multiline) {
    return (
      <TextArea
        id={id}
        invalid={invalid}
        value={draft ?? value}
        placeholder={placeholder}
        spellCheck={!mono}
        onChange={(event) => {
          setDraft(event.target.value);
        }}
        {...events}
      />
    );
  }
  return (
    <TextInput
      id={id}
      mono={mono}
      invalid={invalid}
      value={draft ?? value}
      placeholder={placeholder}
      onChange={(event) => {
        setDraft(event.target.value);
      }}
      onKeyDown={(event) => {
        if (event.key === "Enter") event.currentTarget.blur();
      }}
      {...events}
    />
  );
}

const EntityField = observer(function EntityField({
  editor,
  node,
  param,
  kind,
  label,
  required,
  invalid,
}: {
  readonly editor: ScenarioEditor;
  readonly node: ScenarioNodeDto;
  readonly param: string;
  readonly kind: EntityKind;
  readonly label: string;
  readonly required: boolean;
  readonly invalid: boolean;
}) {
  const options = editor.entities.options(kind, node.params);
  const current = paramText(node.params[param]);
  return (
    <Field label={label} required={required}>
      <SelectInput
        label={label}
        value={current}
        invalid={invalid}
        disabled={options === null}
        placeholder={options === null ? MODEL_NEEDS_PROVIDER : ENTITY_PLACEHOLDERS[kind]}
        options={withCurrent(options ?? [], current)}
        onChange={(value) => {
          editor.updateNode(node.id, { params: withParam(node.params, param, value) });
        }}
      />
    </Field>
  );
});

function ParamField({
  editor,
  node,
  field,
  invalid,
}: {
  readonly editor: ScenarioEditor;
  readonly node: ScenarioNodeDto;
  readonly field: FormField;
  readonly invalid: boolean;
}) {
  const id = `scenario-param-${field.key}`;
  const value = node.params[field.key];
  if (field.kind === "boolean") {
    return (
      <Toggle
        id={id}
        label={field.label}
        checked={value === true}
        onChange={(checked) => {
          editor.updateNode(node.id, { params: { ...node.params, [field.key]: checked } });
        }}
      />
    );
  }
  if (field.kind === "select" && field.options !== undefined) {
    return (
      <Field label={field.label} required={field.required} help={field.hint}>
        <SelectInput
          label={field.label}
          value={paramText(value)}
          invalid={invalid}
          options={withCurrent(
            field.options.map((option) => ({ value: option, label: option })),
            paramText(value),
          )}
          onChange={(next) => {
            editor.updateNode(node.id, { params: withParam(node.params, field.key, next) });
          }}
        />
      </Field>
    );
  }
  const json = field.kind === "json";
  return (
    <Field label={field.label} htmlFor={id} required={field.required} help={field.hint}>
      <CommitInput
        id={id}
        mono={field.kind !== "multiline"}
        multiline={field.kind === "multiline" || json}
        value={json && value !== undefined ? JSON.stringify(value, null, 2) : paramText(value)}
        placeholder={field.placeholder}
        invalid={invalid}
        onCommit={(text) => {
          editor.updateNode(node.id, { params: withField(node.params, field, text) });
        }}
      />
    </Field>
  );
}

const ParamsJson = observer(function ParamsJson({
  editor,
  node,
}: {
  readonly editor: ScenarioEditor;
  readonly node: ScenarioNodeDto;
}) {
  const [draft, setDraft] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const text = JSON.stringify(node.params, null, 2);
  return (
    <Field label="Параметры (JSON)" htmlFor="scenario-node-params" error={error ?? undefined}>
      <TextArea
        id="scenario-node-params"
        value={draft ?? text}
        invalid={error !== null}
        spellCheck={false}
        onFocus={() => {
          setDraft(text);
        }}
        onChange={(event) => {
          setDraft(event.target.value);
        }}
        onBlur={() => {
          const params = parseParams(draft ?? text);
          if (params === null) {
            setError(PARAMS_INVALID);
            return;
          }
          setError(null);
          setDraft(null);
          if (JSON.stringify(params) !== JSON.stringify(node.params)) {
            editor.updateNode(node.id, { params });
          }
        }}
      />
    </Field>
  );
});

function childLine(child: ScenarioNodeDto): string {
  if (child.disabled === true) return "Отключён";
  if (child.type === TOOL_SET_TYPE) {
    const count = toolSetKeys(child.params).length;
    return count === 0 ? TOOL_SET_EMPTY : toolCountLabel(count);
  }
  const model = child.params.modelId;
  return typeof model === "string" && model.length > 0 ? model : child.type;
}

const SubNodes = observer(function SubNodes({
  editor,
  node,
}: {
  readonly editor: ScenarioEditor;
  readonly node: ScenarioNodeDto;
}) {
  const spec = editor.catalogue.get(node.type);
  if (spec === undefined || spec.slots.length === 0) return null;
  return (
    <>
      <div className="h-px flex-none bg-line" />
      <h3 className={SECTION}>Подключённые подузлы</h3>
      {spec.slots.map((entry) => {
        const children = childrenIn(editor.graph, node.id, entry.slot);
        return (
          <div
            key={entry.slot}
            className={`rounded-card border border-line bg-main-850 px-2.75 py-2.5 ${
              children.length === 0 ? "opacity-70" : ""
            }`}
          >
            <div className="flex items-center gap-2.25">
              <StatusDot tone={children.length > 0 ? "ok" : entry.required ? "err" : "idle"} />
              <span className="flex-1 text-[12px] font-medium text-main-100">
                {slotLabel(entry.slot, entry.required)}
              </span>
            </div>
            {children.length === 0 ? (
              <span className="mt-1 block pl-4.25 font-mono text-[10.5px] text-main-500">
                Не подключено
              </span>
            ) : (
              children.map((attachment) => {
                const child = findNode(editor.graph, attachment.childNodeId);
                const broken = (editor.issuesByNode.get(attachment.childNodeId) ?? []).length > 0;
                const tone = child?.disabled === true ? "idle" : broken ? "err" : ("ok" as const);
                return (
                  <div
                    key={attachment.childNodeId}
                    className="mt-1.5 flex items-center gap-2 pl-0.5"
                  >
                    <StatusDot tone={tone} />
                    <button
                      type="button"
                      className="flex min-w-0 flex-1 flex-col text-left"
                      onClick={() => {
                        editor.select(attachment.childNodeId);
                      }}
                    >
                      <span className="truncate text-[11.5px] text-main-200 hover:text-main-50">
                        {editor.nameOf(attachment.childNodeId)}
                      </span>
                      <span className="truncate font-mono text-[10px] text-main-500">
                        {child === undefined ? "" : childLine(child)}
                      </span>
                    </button>
                    <IconButton
                      path={mdiLinkVariantOff}
                      label="Отключить от слота"
                      tooltip="left-center"
                      iconSize={14}
                      className="size-6"
                      onClick={() => {
                        editor.dispatch({
                          type: "remove",
                          nodeIds: [],
                          linkIds: [attachmentLinkId(attachment)],
                        });
                      }}
                    />
                  </div>
                );
              })
            )}
          </div>
        );
      })}
    </>
  );
});

const ParamsTab = observer(function ParamsTab({
  editor,
  node,
}: {
  readonly editor: ScenarioEditor;
  readonly node: ScenarioNodeDto;
}) {
  const spec = editor.catalogue.get(node.type);
  const issues = editor.issuesByNode.get(node.id) ?? [];
  const fields = formFields(spec, node.type);
  const description = descriptionOf(node.type);
  const [showJson, setShowJson] = useState(false);
  const toolSet = node.type === TOOL_SET_TYPE;
  const ceiling = editor.ceilingOf(node.id);
  const missing = (key: string) => issues.some((issue) => issue.param === key);
  const run = editor.execution.nodes.get(node.id);

  return (
    <div className="flex flex-col gap-3.5 p-3.5">
      {description === null ? null : (
        <p className="m-0 text-[12px] leading-relaxed text-main-400">{description}</p>
      )}
      {run?.status === "failed" && run.error !== null ? (
        <p role="alert" className="m-0 rounded-control bg-err-soft px-3 py-2 text-[12px] text-err">
          {`${STEP_STATUS_LABELS.failed}: ${run.error}`}
        </p>
      ) : null}
      <Field label="Название" htmlFor="scenario-node-name">
        <CommitInput
          id="scenario-node-name"
          value={node.name ?? ""}
          placeholder={editor.labelOf(node.type)}
          onCommit={(name) => {
            editor.updateNode(node.id, { name });
          }}
        />
      </Field>

      {fields.map((field) => {
        if (toolSet && field.key === TOOL_SET_PARAM) return null;
        const kind = entityKindOf(field.key);
        if (kind !== null) {
          return (
            <EntityField
              key={field.key}
              editor={editor}
              node={node}
              param={field.key}
              kind={kind}
              label={field.label}
              required={field.required}
              invalid={missing(field.key)}
            />
          );
        }
        return (
          <ParamField
            key={field.key}
            editor={editor}
            node={node}
            field={field}
            invalid={missing(field.key)}
          />
        );
      })}

      {toolSet ? (
        <ToolSetPicker editor={editor} node={node} invalid={missing(TOOL_SET_PARAM)} />
      ) : null}

      <button
        type="button"
        aria-expanded={showJson}
        className="flex items-center gap-1.5 self-start text-[12px] text-main-400 hover:text-main-100"
        onClick={() => {
          setShowJson(!showJson);
        }}
      >
        <Icon path={showJson ? mdiChevronDown : mdiChevronRight} size={16} />
        Все параметры в JSON
      </button>
      {showJson ? <ParamsJson editor={editor} node={node} /> : null}

      {ceiling === null ? null : (
        <p className="m-0 rounded-control bg-warn-soft px-3 py-2 text-[12px] text-warn">
          {ceiling}
        </p>
      )}

      <SubNodes editor={editor} node={node} />

      {issues.length === 0 ? null : (
        <>
          <div className="h-px flex-none bg-line" />
          <h3 className={SECTION}>Ошибки</h3>
          <ul className="m-0 flex list-none flex-col gap-1.5 p-0">
            {issues.map((issue) => (
              <li
                key={`${issue.rule}-${issue.slot ?? ""}-${issue.param ?? ""}`}
                className="text-[12px] leading-snug text-err"
              >
                {issueText(issue, editor.nameOf)}
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  );
});

function NumberSetting({
  id,
  label,
  help,
  value,
  placeholder,
  onCommit,
}: {
  readonly id: string;
  readonly label: string;
  readonly help?: string;
  readonly value: number | undefined;
  readonly placeholder: string;
  readonly onCommit: (value: number | undefined) => void;
}) {
  return (
    <Field label={label} htmlFor={id} help={help}>
      <CommitInput
        id={id}
        mono
        value={value === undefined ? "" : String(value)}
        placeholder={placeholder}
        onCommit={(text) => {
          onCommit(wholeNumber(text));
        }}
      />
    </Field>
  );
}

const PinnedInput = observer(function PinnedInput({
  editor,
  node,
}: {
  readonly editor: ScenarioEditor;
  readonly node: ScenarioNodeDto;
}) {
  const [draft, setDraft] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const text = node.pinnedInput === undefined ? "" : JSON.stringify(node.pinnedInput, null, 2);
  return (
    <Field
      label="Закреплённые входные данные"
      htmlFor="scenario-node-pinned"
      help="Используются при запуске этого шага. Пусто — берётся результат предыдущего шага из последнего запуска."
      error={error ?? undefined}
      labelAction={
        node.pinnedInput === undefined ? null : (
          <button
            type="button"
            className="ml-auto text-[11.5px] font-normal text-main-400 hover:text-main-100"
            onClick={() => {
              setError(null);
              editor.pinInput(node.id, null);
            }}
          >
            Сбросить
          </button>
        )
      }
    >
      <TextArea
        id="scenario-node-pinned"
        value={draft ?? text}
        invalid={error !== null}
        spellCheck={false}
        placeholder="{}"
        onFocus={() => {
          setDraft(text);
        }}
        onChange={(event) => {
          setDraft(event.target.value);
        }}
        onBlur={() => {
          const parsed = parseJson(draft ?? text);
          if (parsed === null) {
            setError(PINNED_INVALID);
            return;
          }
          setError(null);
          setDraft(null);
          if ((draft ?? text) !== text) editor.pinInput(node.id, parsed.value);
        }}
      />
    </Field>
  );
});

const SettingsTab = observer(function SettingsTab({
  editor,
  node,
}: {
  readonly editor: ScenarioEditor;
  readonly node: ScenarioNodeDto;
}) {
  const settings = node.settings ?? {};
  const change = (patch: Partial<ScenarioNodeSettingsDto>) => {
    const next: ScenarioNodeSettingsDto = { ...settings, ...patch };
    for (const key of Object.keys(next) as (keyof ScenarioNodeSettingsDto)[]) {
      if (next[key] === undefined) delete next[key];
    }
    editor.setSettings(node.id, next);
  };
  const timeout = settings.timeoutMs === undefined ? undefined : settings.timeoutMs / 1000;
  return (
    <div className="flex flex-col gap-3.5 p-3.5">
      <div className="grid grid-cols-2 gap-3">
        <NumberSetting
          id="scenario-node-attempts"
          label="Попыток"
          value={settings.maxAttempts}
          placeholder="1"
          onCommit={(maxAttempts) => {
            change({
              maxAttempts:
                maxAttempts === undefined ? undefined : Math.min(10, Math.max(1, maxAttempts)),
            });
          }}
        />
        <NumberSetting
          id="scenario-node-backoff"
          label="Пауза, мс"
          value={settings.backoffMs}
          placeholder="0"
          onCommit={(backoffMs) => {
            change({
              backoffMs:
                backoffMs === undefined ? undefined : Math.min(60_000, Math.max(0, backoffMs)),
            });
          }}
        />
      </div>
      <NumberSetting
        id="scenario-node-timeout"
        label="Ограничение времени, с"
        help="Пусто — без ограничения."
        value={timeout}
        placeholder="—"
        onCommit={(seconds) => {
          change({
            timeoutMs:
              seconds === undefined ? undefined : Math.min(3_600, Math.max(1, seconds)) * 1000,
          });
        }}
      />
      <Field label="При ошибке">
        <SelectInput
          label="При ошибке"
          value={settings.onError ?? "stop"}
          options={ON_ERROR_OPTIONS}
          onChange={(value) => {
            change({ onError: value === "continue" ? "continue" : undefined });
          }}
        />
      </Field>
      <div className="h-px flex-none bg-line" />
      <PinnedInput editor={editor} node={node} />
    </div>
  );
});

const NotesTab = observer(function NotesTab({
  editor,
  node,
}: {
  readonly editor: ScenarioEditor;
  readonly node: ScenarioNodeDto;
}) {
  return (
    <div className="flex flex-col gap-3.5 p-3.5">
      <Field label="Заметки" htmlFor="scenario-node-notes">
        <CommitInput
          id="scenario-node-notes"
          multiline
          value={node.notes ?? ""}
          onCommit={(notes) => {
            editor.setNotes(node.id, notes);
          }}
        />
      </Field>
    </div>
  );
});

function SelectionActions({ editor }: { readonly editor: ScenarioEditor }) {
  return (
    <div className="flex flex-none gap-2 border-t border-line px-3.5 py-3">
      <Button tone="secondary" onClick={editor.duplicateSelection}>
        <Icon path={mdiContentCopy} size={15} />
        Дублировать
      </Button>
      <span className="flex-1" />
      <Button tone="danger" onClick={editor.deleteSelection}>
        <Icon path={mdiDeleteOutline} size={15} />
        Удалить
      </Button>
    </div>
  );
}

const NodeActions = observer(function NodeActions({
  editor,
  node,
  runnable,
}: {
  readonly editor: ScenarioEditor;
  readonly node: ScenarioNodeDto;
  readonly runnable: boolean;
}) {
  return (
    <div className="flex flex-none items-center gap-2 border-t border-line px-3.5 py-3">
      {runnable ? (
        <span className="flex flex-1 [&>button]:w-full">
          <Button
            tone="secondary"
            disabled={editor.execution.running}
            onClick={() => void editor.runNode(node.id)}
          >
            <Icon path={mdiPlayOutline} size={15} />
            Запустить шаг
          </Button>
        </span>
      ) : (
        <span className="flex-1" />
      )}
      <Button
        tone="ghost"
        onClick={() => {
          editor.toggleDisabled(node.id);
        }}
      >
        <Icon path={node.disabled === true ? mdiPlay : mdiCancel} size={15} />
        {node.disabled === true ? "Включить" : "Отключить"}
      </Button>
      <IconButton
        path={mdiDeleteOutline}
        label="Удалить шаг"
        tooltip="top-right"
        iconSize={16}
        className="size-8"
        onClick={() => {
          editor.deleteNode(node.id);
        }}
      />
    </div>
  );
});

const NodeForm = observer(function NodeForm({
  editor,
  node,
}: {
  readonly editor: ScenarioEditor;
  readonly node: ScenarioNodeDto;
}) {
  const spec = editor.catalogue.get(node.type);
  const step = spec?.role !== "subNode";
  const tabs: readonly InspectorTab[] = step
    ? ["params", "settings", "notes"]
    : ["params", "notes"];
  const [tab, setTab] = useState<InspectorTab>("params");
  const active = tabs.includes(tab) ? tab : "params";
  return (
    <>
      <div className="flex h-10.5 flex-none items-center gap-1.25 border-b border-line px-3">
        {tabs.map((entry) => (
          <button
            key={entry}
            type="button"
            aria-pressed={active === entry}
            className={`inline-flex h-7 items-center rounded-[6px] px-3 text-[12.5px] font-medium ${
              active === entry ? "bg-main-700 text-main-50" : "text-main-400 hover:text-main-100"
            }`}
            onClick={() => {
              setTab(entry);
            }}
          >
            {TAB_LABELS[entry]}
          </button>
        ))}
      </div>
      <ScrollArea className="min-h-0 flex-1">
        {active === "params" ? <ParamsTab editor={editor} node={node} /> : null}
        {active === "settings" ? <SettingsTab editor={editor} node={node} /> : null}
        {active === "notes" ? <NotesTab editor={editor} node={node} /> : null}
      </ScrollArea>
      <NodeActions editor={editor} node={node} runnable={step} />
    </>
  );
});

const Overview = observer(function Overview({ editor }: { readonly editor: ScenarioEditor }) {
  const count = editor.selection.nodes.length;
  if (count > 1) {
    return (
      <div className="flex flex-1 flex-col">
        <div className="flex-1 p-3.5 text-[12.5px] text-main-400">Выбрано: {stepsWord(count)}</div>
        <SelectionActions editor={editor} />
      </div>
    );
  }
  const scenario = editor.scenario;
  return (
    <ScrollArea className="min-h-0 flex-1">
      <div className="flex flex-col gap-3 p-3.5">
        <h3 className={SECTION}>Проверка сценария</h3>
        {editor.issues.length === 0 ? (
          <p className="m-0 flex items-center gap-2 text-[12.5px] text-main-300">
            <StatusDot tone="ok" />
            Ошибок нет
          </p>
        ) : (
          <ul className="m-0 flex list-none flex-col gap-1 p-0">
            {editor.issues.map((issue, index) => (
              <li key={`${issue.rule}-${issue.nodeId ?? ""}-${String(index)}`}>
                <button
                  type="button"
                  disabled={issue.nodeId === undefined}
                  className="w-full rounded-control px-2 py-1.5 text-left text-[12px] leading-snug text-err enabled:hover:bg-err-soft"
                  onClick={() => {
                    if (issue.nodeId !== undefined) editor.select(issue.nodeId);
                  }}
                >
                  {issueText(issue, editor.nameOf)}
                </button>
              </li>
            ))}
          </ul>
        )}
        {scenario === null ? null : (
          <>
            <div className="h-px flex-none bg-line" />
            <h3 className={SECTION}>Запуски</h3>
            <Field label="Пересечение запусков" help={OVERLAP_HINT}>
              <SelectInput
                label="Пересечение запусков"
                value={scenario.overlap}
                disabled={editor.overlapSaving}
                options={OVERLAP_OPTIONS}
                onChange={(value) => {
                  void editor.setOverlap(value === "queue" ? "queue" : "skip");
                }}
              />
            </Field>
          </>
        )}
      </div>
    </ScrollArea>
  );
});

function NodeInspector({ editor }: { readonly editor: ScenarioEditor }) {
  const node = editor.selectedNode;
  const spec = node === null ? undefined : editor.catalogue.get(node.type);
  return (
    <aside className="flex w-85 flex-none flex-col border-l border-line bg-main-900">
      <div className="flex h-13 flex-none items-center gap-2.5 border-b border-line px-3.5">
        {node === null ? (
          <span className="flex-1 text-[13px] font-semibold text-main-50">Сценарий</span>
        ) : (
          <>
            <span className="flex size-7 flex-none items-center justify-center rounded-lg bg-main-700 text-accent-medium">
              <Icon path={iconFor(node.type, spec?.toolKey ?? null)} size={16} />
            </span>
            <span className="flex min-w-0 flex-1 flex-col">
              <span className="truncate text-[13px] font-semibold text-main-50">
                {editor.nameOf(node.id)}
              </span>
              <span className="truncate text-[10.5px] text-main-500">
                {spec === undefined ? node.type : `${ROLE_LABELS[spec.role]} · ${node.type}`}
              </span>
            </span>
            <IconButton
              path={mdiClose}
              label="Снять выделение"
              tooltip="bottom-right"
              iconSize={16}
              onClick={editor.clearSelection}
            />
          </>
        )}
      </div>
      {node === null ? (
        <Overview editor={editor} />
      ) : (
        <NodeForm key={node.id} editor={editor} node={node} />
      )}
    </aside>
  );
}

export default observer(NodeInspector);
