import { observer } from "mobx-react-lite";
import { Modal } from "@kiyotakkkka/zvs-uikit-lib";
import {
  AGENTIC_DEFAULT_MINUTES,
  AGENTIC_DEFAULT_STEPS,
  AGENTIC_DEFAULT_TOKENS,
  AGENTIC_GOAL_MAX,
  AgenticPermissionMode,
} from "@zvs/shared";
import ButtonBase from "../../ui/atoms/buttons/ButtonBase";
import SegmentedControl from "../../ui/atoms/SegmentedControl";
import SelectInput from "../../ui/atoms/SelectInput";
import TextArea from "../../ui/atoms/TextArea";
import TextInput from "../../ui/atoms/TextInput";
import Alert from "../../ui/molecules/Alert";
import Field from "../../ui/molecules/Field";
import {
  CANCEL,
  FORM_BUDGET,
  FORM_BUDGET_HELP,
  FORM_GOAL,
  FORM_GOAL_PLACEHOLDER,
  FORM_MINUTES,
  FORM_MODEL,
  FORM_NO_FOLDER,
  FORM_NO_PROVIDERS,
  FORM_PICK_FOLDER,
  FORM_PROVIDER,
  FORM_STEPS,
  FORM_TOKENS,
  FORM_WORKSPACE,
  FORM_WORKSPACE_HELP,
  MODE_HINTS,
  MODE_LABELS,
  MODE_TITLE,
  NEW_SESSION,
  START_SESSION,
} from "./agenticCopy";
import type AgenticStore from "./AgenticStore";
import type { BudgetField } from "./SessionDraft";

const MODE_OPTIONS = AgenticPermissionMode.options.map((value) => ({
  value,
  label: MODE_LABELS[value],
}));

const BUDGET_FIELDS: readonly { key: BudgetField; label: string; fallback: number }[] = [
  { key: "steps", label: FORM_STEPS, fallback: AGENTIC_DEFAULT_STEPS },
  { key: "tokens", label: FORM_TOKENS, fallback: AGENTIC_DEFAULT_TOKENS },
  { key: "minutes", label: FORM_MINUTES, fallback: AGENTIC_DEFAULT_MINUTES },
];

export default observer(function NewSessionModal({ store }: { readonly store: AgenticStore }) {
  const draft = store.draft;
  const provider = store.providers.find((entry) => entry.id === draft.providerId);
  return (
    <Modal
      open={store.formOpen}
      onClose={store.closeForm}
      label={NEW_SESSION}
      className="w-150 max-w-[92vw] overflow-hidden rounded-card border border-line bg-main-900 p-0 shadow-pop"
    >
      <div className="flex flex-col gap-4 px-5 pt-5">
        <h2 className="m-0 text-[15px] font-semibold tracking-[-0.01em] text-main-50">
          {NEW_SESSION}
        </h2>
        <Field label={FORM_GOAL} required>
          <TextArea
            aria-label={FORM_GOAL}
            placeholder={FORM_GOAL_PLACEHOLDER}
            value={draft.goal}
            maxLength={AGENTIC_GOAL_MAX}
            minRows={3}
            maxRows={8}
            autoFocus
            onChange={(event) => {
              draft.setGoal(event.target.value);
            }}
          />
        </Field>
        <Field label={FORM_WORKSPACE} required help={FORM_WORKSPACE_HELP}>
          <div className="flex items-center gap-2">
            <span
              className={`min-w-0 flex-1 truncate rounded-control border border-line bg-main-850 px-3 py-2 font-mono text-[12px] ${draft.workspace === null ? "text-main-500" : "text-main-100"}`}
              title={draft.workspace ?? undefined}
            >
              {draft.workspace ?? FORM_NO_FOLDER}
            </span>
            <ButtonBase disabled={store.picking} onClick={() => void store.pickFolder()}>
              {FORM_PICK_FOLDER}
            </ButtonBase>
          </div>
        </Field>
        {store.providers.length === 0 ? (
          <Alert tone="info">{FORM_NO_PROVIDERS}</Alert>
        ) : (
          <div className="grid grid-cols-2 gap-3">
            <Field label={FORM_PROVIDER}>
              <SelectInput
                label={FORM_PROVIDER}
                value={draft.providerId}
                options={store.providers.map((entry) => ({ value: entry.id, label: entry.name }))}
                onChange={(id) => {
                  const next = store.providers.find((entry) => entry.id === id);
                  if (next !== undefined) draft.selectProvider(next);
                }}
              />
            </Field>
            <Field label={FORM_MODEL}>
              <SelectInput
                label={FORM_MODEL}
                value={draft.modelId}
                options={(provider?.models ?? [])
                  .filter((model) => model.available)
                  .map((model) => ({
                    value: model.externalId,
                    label: model.displayName.length > 0 ? model.displayName : model.externalId,
                  }))}
                onChange={draft.setModel}
              />
            </Field>
          </div>
        )}
        <Field label={MODE_TITLE} help={MODE_HINTS[draft.mode]}>
          <SegmentedControl
            label={MODE_TITLE}
            value={draft.mode}
            options={MODE_OPTIONS}
            onChange={draft.setMode}
          />
        </Field>
        <Field
          label={FORM_BUDGET}
          optionalHint
          help={FORM_BUDGET_HELP}
          {...(draft.budgetInvalid ? { error: FORM_BUDGET_HELP } : {})}
        >
          <div className="grid grid-cols-3 gap-3">
            {BUDGET_FIELDS.map((field) => (
              <label key={field.key} className="flex flex-col gap-1 text-[11.5px] text-main-400">
                {field.label}
                <TextInput
                  mono
                  inputMode="numeric"
                  aria-label={field.label}
                  placeholder={field.fallback.toLocaleString("ru-RU")}
                  value={draft.budget[field.key]}
                  invalid={draft.budgetInvalid && draft.budget[field.key].length > 0}
                  onChange={(event) => {
                    draft.setBudget(field.key, event.target.value);
                  }}
                />
              </label>
            ))}
          </div>
        </Field>
        {store.formError === null ? null : <Alert>{store.formError}</Alert>}
      </div>
      <div className="mt-5 flex justify-end gap-2 bg-main-850 px-5 py-3">
        <ButtonBase onClick={store.closeForm}>{CANCEL}</ButtonBase>
        <ButtonBase tone="primary" disabled={!store.canCreate} onClick={() => void store.create()}>
          {START_SESSION}
        </ButtonBase>
      </div>
    </Modal>
  );
});
