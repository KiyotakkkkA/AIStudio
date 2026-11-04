import { useEffect, useMemo, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import {
  mdiAlertCircleOutline,
  mdiCancel,
  mdiCheck,
  mdiChevronDown,
  mdiChevronUp,
  mdiClockOutline,
  mdiLoading,
  mdiMonitor,
  mdiSendOutline,
  mdiStop,
} from "@mdi/js";
import type {
  ComputerAction,
  ComputerApproval,
  ComputerAppDto,
  ComputerCommand,
  ComputerState,
} from "@zvs/shared";
import "../renderer/app/theme.css";
import "./style.css";
import Icon from "../browser-ui/Icon";
import { applyChromeAppearance } from "../browser-ui/appearance";
import { GLOW_LABEL, sendCommand, subscribeState, windowLabel } from "./bridge";
import {
  ACCESS_ANSWERS,
  ACCESS_TITLE,
  ACTION_STATE_TEXT,
  ACTION_TITLE,
  ACTIONS_EMPTY,
  ALLOW,
  BADGE_TEXT,
  COLLAPSE,
  COMMAND_FAILED,
  COMPOSER_PLACEHOLDER,
  DENY,
  EXPAND,
  PANEL_TITLE,
  SEND,
  STATUS_TEXT,
  STOP,
  STOP_HINT,
} from "./computerCopy";

const DARK_SCHEME_QUERY = "(prefers-color-scheme: dark)";
const ACTIONS_SHOWN = 8;

const INITIAL: ComputerState = {
  active: false,
  status: "idle",
  apps: [],
  actions: [],
  collapsed: false,
  appearance: { theme: "system", darkPalette: "zvs-dark", lightPalette: "zvs-light" },
};

const ACTION_ICONS: Record<ComputerAction["state"], string> = {
  running: mdiLoading,
  waiting: mdiClockOutline,
  done: mdiCheck,
  failed: mdiAlertCircleOutline,
  denied: mdiCancel,
};

function useOsDark(): boolean {
  const [dark, setDark] = useState(() => window.matchMedia(DARK_SCHEME_QUERY).matches);
  useEffect(() => {
    const list = window.matchMedia(DARK_SCHEME_QUERY);
    const sync = () => setDark(list.matches);
    list.addEventListener("change", sync);
    return () => list.removeEventListener("change", sync);
  }, []);
  return dark;
}

function useComputerState(): ComputerState {
  const [state, setState] = useState<ComputerState>(INITIAL);
  const osDark = useOsDark();
  useEffect(() => subscribeState(setState), []);
  useEffect(() => applyChromeAppearance(state.appearance, osDark), [state.appearance, osDark]);
  return state;
}

function useIconUrl(icon: string | undefined): string | null {
  const url = useMemo(() => {
    if (icon === undefined || icon.length === 0) return null;
    try {
      const bytes = Uint8Array.from(atob(icon), (char) => char.charCodeAt(0));
      return URL.createObjectURL(new Blob([bytes], { type: "image/png" }));
    } catch {
      return null;
    }
  }, [icon]);
  useEffect(
    () => () => {
      if (url !== null) URL.revokeObjectURL(url);
    },
    [url],
  );
  return url;
}

function AppRow({ app }: { readonly app: ComputerAppDto }) {
  const url = useIconUrl(app.icon);
  return (
    <li className="app-row">
      {url === null ? (
        <span className="app-icon app-icon-empty">
          <Icon path={mdiMonitor} size={16} />
        </span>
      ) : (
        <img className="app-icon" src={url} alt="" />
      )}
      <span className="app-text">
        <span className="app-name">{app.app}</span>
        {app.path.length > 0 ? (
          <span className="app-path" title={app.path}>
            {app.path}
          </span>
        ) : null}
      </span>
    </li>
  );
}

function ApprovalBlock({
  approval,
  run,
}: {
  readonly approval: ComputerApproval;
  readonly run: (command: ComputerCommand) => void;
}) {
  const access = approval.kind === "access";
  return (
    <section className="approval" aria-label={access ? ACCESS_TITLE : ACTION_TITLE}>
      <h2>{access ? ACCESS_TITLE : ACTION_TITLE}</h2>
      {approval.detail === undefined ? null : <p className="approval-detail">{approval.detail}</p>}
      {approval.apps.length === 0 ? null : (
        <ul className="app-list">
          {approval.apps.map((app) => (
            <AppRow key={app.app} app={app} />
          ))}
        </ul>
      )}
      <div className="approval-actions">
        {access ? (
          ACCESS_ANSWERS.map(({ answer, label }) => (
            <button
              key={answer}
              type="button"
              className={answer === "deny" ? "danger-soft" : answer === "once" ? "primary" : ""}
              onClick={() => run({ action: "answer", id: approval.id, answer })}
            >
              {label}
            </button>
          ))
        ) : (
          <>
            <button
              type="button"
              className="primary"
              onClick={() => run({ action: "approve", id: approval.id })}
            >
              {ALLOW}
            </button>
            <button
              type="button"
              className="danger-soft"
              onClick={() => run({ action: "deny", id: approval.id })}
            >
              {DENY}
            </button>
          </>
        )}
      </div>
    </section>
  );
}

function Composer({ run }: { readonly run: (command: ComputerCommand) => void }) {
  const [text, setText] = useState("");
  const field = useRef<HTMLTextAreaElement>(null);
  const submit = () => {
    const value = text.trim();
    if (value.length === 0) return;
    setText("");
    field.current?.blur();
    run({ action: "say", text: value });
  };
  return (
    <form
      className="composer"
      onSubmit={(event) => {
        event.preventDefault();
        submit();
      }}
    >
      <textarea
        ref={field}
        rows={2}
        value={text}
        placeholder={COMPOSER_PLACEHOLDER}
        onPointerDown={() => run({ action: "engage" })}
        onFocus={() => run({ action: "engage" })}
        onBlur={() => run({ action: "release" })}
        onChange={(event) => setText(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter" && !event.shiftKey) {
            event.preventDefault();
            submit();
          }
          if (event.key === "Escape") field.current?.blur();
        }}
      />
      <button type="submit" className="icon-button" aria-label={SEND} disabled={text.trim() === ""}>
        <Icon path={mdiSendOutline} size={16} />
      </button>
    </form>
  );
}

function Panel({ state }: { readonly state: ComputerState }) {
  const [error, setError] = useState<string | null>(null);
  const run = (command: ComputerCommand) => {
    setError(null);
    sendCommand(command).catch((reason: unknown) => {
      setError(typeof reason === "string" ? reason : COMMAND_FAILED);
    });
  };
  const actions = state.actions.slice(-ACTIONS_SHOWN).reverse();
  return (
    <main className={`panel ${state.collapsed ? "collapsed" : ""}`}>
      <header className="panel-header">
        <span className={`pulse status-${state.status}`} aria-hidden="true" />
        <span className="panel-heading">
          <span className="panel-title">{PANEL_TITLE}</span>
          <span className="panel-subtitle">{state.title ?? STATUS_TEXT[state.status]}</span>
        </span>
        <button
          type="button"
          className="icon-button"
          aria-label={state.collapsed ? EXPAND : COLLAPSE}
          onClick={() => run({ action: "collapse", collapsed: !state.collapsed })}
        >
          <Icon path={state.collapsed ? mdiChevronUp : mdiChevronDown} size={16} />
        </button>
        <button type="button" className="danger" onClick={() => run({ action: "stop" })}>
          <Icon path={mdiStop} size={14} />
          {STOP}
        </button>
      </header>
      {state.collapsed ? null : (
        <>
          <p className="hint">
            {STATUS_TEXT[state.status]} · {STOP_HINT}
          </p>
          {state.approval === undefined ? null : (
            <ApprovalBlock approval={state.approval} run={run} />
          )}
          <ol className="actions" aria-live="polite">
            {actions.length === 0 ? <li className="actions-empty">{ACTIONS_EMPTY}</li> : null}
            {actions.map((action) => (
              <li key={action.id} className={`action action-${action.state}`}>
                <span className="action-mark" title={ACTION_STATE_TEXT[action.state]}>
                  <Icon path={ACTION_ICONS[action.state]} size={14} />
                </span>
                <span className="action-text">
                  <span className="action-label">{action.label}</span>
                  {action.error === undefined ? null : (
                    <span className="action-error">{action.error}</span>
                  )}
                </span>
              </li>
            ))}
          </ol>
          {error === null ? null : <p className="panel-error">{error}</p>}
          <Composer run={run} />
        </>
      )}
    </main>
  );
}

function Glow({ state }: { readonly state: ComputerState }) {
  return (
    <div className={`glow status-${state.status}`}>
      <div className="badge" role="status">
        <span className={`pulse status-${state.status}`} aria-hidden="true" />
        <span className="badge-text">{BADGE_TEXT}</span>
        <span className="badge-status">{STATUS_TEXT[state.status]}</span>
        <kbd>{STOP_HINT}</kbd>
      </div>
    </div>
  );
}

function ComputerApp() {
  const state = useComputerState();
  return windowLabel() === GLOW_LABEL ? <Glow state={state} /> : <Panel state={state} />;
}

createRoot(document.getElementById("root")!).render(<ComputerApp />);
