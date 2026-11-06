import { observer } from "mobx-react-lite";
import { useEffect } from "react";
import { mdiClose, mdiPlus } from "@mdi/js";
import {
  BROWSER_SITE_ACTIONS,
  BROWSER_SITE_DECISIONS,
  type BrowserSiteAction,
  type BrowserSiteDecision,
  type BrowserSiteRulesDto,
} from "@zvs/shared";
import Button from "../../ui/atoms/buttons/Button";
import IconButton from "../../ui/atoms/buttons/IconButton";
import Icon from "../../ui/atoms/Icon";
import SelectInput from "../../ui/atoms/SelectInput";
import TextInput from "../../ui/atoms/TextInput";
import Alert from "../../ui/molecules/Alert";
import PreferenceContainer from "../../ui/molecules/PreferenceContainer";
import { useStore } from "../../stores/useStore";
import {
  SITE_ACTION_HINTS,
  SITE_ACTION_LABELS,
  SITE_ADD_CANCEL,
  SITE_ADD_CONFIRM,
  SITE_COLUMN,
  SITE_DECISION_LABELS,
  SITE_DEFAULT_ROW,
  SITE_INPUT_LABEL,
  SITE_PLACEHOLDER,
  SITE_REMOVE,
  SITE_REMOVE_TITLE,
  SITE_USE_DEFAULT,
  SITES_ADD,
  SITES_EMPTY,
  SITES_HINT,
  SITES_ID,
  SITES_TITLE,
  siteHost,
  siteRemoveConsequence,
} from "./browserSitesCopy";

const GRID = "grid grid-cols-[minmax(9rem,1.3fr)_repeat(5,minmax(6.5rem,1fr))_2rem] gap-2";
const HEAD = "text-[11px] font-medium text-main-500";

const DECISION_OPTIONS = BROWSER_SITE_DECISIONS.map((decision) => ({
  value: decision,
  label: SITE_DECISION_LABELS[decision],
}));

const RULE_OPTIONS = [{ value: "", label: SITE_USE_DEFAULT }, ...DECISION_OPTIONS];

function decisionOf(value: string): BrowserSiteDecision | null {
  return BROWSER_SITE_DECISIONS.find((decision) => decision === value) ?? null;
}

function RuleCells({
  rules,
  disabled,
  onChange,
}: {
  readonly rules: BrowserSiteRulesDto;
  readonly disabled: boolean;
  readonly onChange: (action: BrowserSiteAction, decision: BrowserSiteDecision | null) => void;
}) {
  return (
    <>
      {BROWSER_SITE_ACTIONS.map((action) => (
        <SelectInput
          key={action}
          label={SITE_ACTION_LABELS[action]}
          value={rules[action] ?? ""}
          options={RULE_OPTIONS}
          disabled={disabled}
          onChange={(value) => onChange(action, decisionOf(value))}
          menuPlacement="bottom-right"
        />
      ))}
    </>
  );
}

const AddRow = observer(function AddRow() {
  const { browserSites } = useStore();
  return (
    <form
      className="flex flex-col gap-3 border-b border-line px-4 py-3"
      onSubmit={(event) => {
        event.preventDefault();
        void browserSites.addDraft();
      }}
    >
      <div className={`${GRID} items-center`}>
        <TextInput
          aria-label={SITE_INPUT_LABEL}
          mono
          autoFocus
          placeholder={SITE_PLACEHOLDER}
          value={browserSites.draftSite}
          onChange={(event) => browserSites.setDraftSite(event.target.value)}
        />
        <RuleCells
          rules={browserSites.draftRules}
          disabled={browserSites.busy}
          onChange={browserSites.setDraftRule}
        />
        <span />
      </div>
      <div className="flex justify-end gap-2">
        <Button tone="ghost" size="sm" onClick={browserSites.cancelAdding}>
          {SITE_ADD_CANCEL}
        </Button>
        <Button type="submit" tone="primary" size="sm" disabled={!browserSites.canAdd}>
          {SITE_ADD_CONFIRM}
        </Button>
      </div>
    </form>
  );
});

export default observer(function SitePermissions() {
  const { browserSites, ui } = useStore();
  useEffect(() => {
    void browserSites.load();
  }, [browserSites]);
  const { defaults, sites } = browserSites.sites;
  return (
    <PreferenceContainer
      title={SITES_TITLE}
      hint={SITES_HINT}
      bare
      action={
        <Button
          tone="secondary"
          size="sm"
          disabled={browserSites.adding}
          onClick={browserSites.startAdding}
        >
          <Icon path={mdiPlus} size={14} />
          {SITES_ADD}
        </Button>
      }
    >
      <div
        data-setting={SITES_ID}
        className={`flex flex-col rounded-card border border-line bg-main-850 transition-colors duration-500 ${
          ui.preferenceTarget === SITES_ID ? "bg-accent-soft" : ""
        }`}
      >
        {browserSites.error === null ? null : (
          <Alert className="mx-4 mt-3" onDismiss={browserSites.dismissError}>
            {browserSites.error}
          </Alert>
        )}
        <div className={`${GRID} items-end border-b border-line px-4 pt-3 pb-2`}>
          <span className={HEAD}>{SITE_COLUMN}</span>
          {BROWSER_SITE_ACTIONS.map((action) => (
            <span key={action} className={HEAD} title={SITE_ACTION_HINTS[action]}>
              {SITE_ACTION_LABELS[action]}
            </span>
          ))}
          <span />
        </div>
        {browserSites.adding ? <AddRow /> : null}
        <div className={`${GRID} items-center border-b border-line px-4 py-2.5`}>
          <span className="text-[13px] font-medium text-main-100">{SITE_DEFAULT_ROW}</span>
          {BROWSER_SITE_ACTIONS.map((action) => (
            <SelectInput
              key={action}
              label={`${SITE_DEFAULT_ROW}: ${SITE_ACTION_LABELS[action]}`}
              value={defaults[action]}
              options={DECISION_OPTIONS}
              disabled={browserSites.busy}
              onChange={(value) => {
                const decision = decisionOf(value);
                if (decision !== null) void browserSites.setDefault(action, decision);
              }}
              menuPlacement="bottom-right"
            />
          ))}
          <span />
        </div>
        {sites.length === 0 ? (
          <p className="m-0 px-4 py-3 text-[12px] text-main-500">{SITES_EMPTY}</p>
        ) : (
          sites.map((site) => (
            <div
              key={site.origin}
              className={`${GRID} items-center border-b border-line px-4 py-2.5 last:border-b-0`}
            >
              <span className="truncate font-mono text-[12px] text-main-100" title={site.origin}>
                {siteHost(site.origin)}
              </span>
              <RuleCells
                rules={site.rules}
                disabled={browserSites.busy}
                onChange={(action, decision) =>
                  void browserSites.setRule(site.origin, action, decision)
                }
              />
              <IconButton
                path={mdiClose}
                label={SITE_REMOVE}
                size="sm"
                tone="danger"
                disabled={browserSites.busy}
                needConfirm
                modalSetup={{
                  title: SITE_REMOVE_TITLE,
                  content: siteRemoveConsequence(siteHost(site.origin)),
                  confirmLabel: SITE_REMOVE,
                }}
                onClick={() => void browserSites.remove(site.origin)}
              />
            </div>
          ))
        )}
      </div>
    </PreferenceContainer>
  );
});
