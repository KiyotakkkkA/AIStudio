import type { BrowserAgentAction, BrowserAgentState, BrowserCommand } from "@zvs/shared";
import {
  mdiAlertOutline,
  mdiCheck,
  mdiClose,
  mdiHandBackRightOutline,
  mdiLinkVariantOff,
  mdiPauseCircleOutline,
  mdiPlay,
  mdiProgressClock,
  mdiStop,
} from "@mdi/js";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import Icon from "./Icon";
import IconButton from "./IconButton";
import {
  ACTIONS_TITLE,
  AGENT_CLOSE,
  AGENT_TITLE,
  NO_ACTIONS,
  RESUME,
  REVOKE,
  RIGHTS_TITLE,
  STATUS_LABELS,
  STATUS_TONES,
  STOP,
  TAKE_CONTROL,
  TARGET_LABEL,
  TASK_TITLE,
  actionMeta,
  actionTitle,
  canResume,
  canStop,
  canTakeControl,
  siteName,
  siteRights,
} from "./agentCopy";

const ACTION_ICONS: Record<BrowserAgentAction["state"], string> = {
  running: mdiProgressClock,
  waiting: mdiAlertOutline,
  done: mdiCheck,
  failed: mdiClose,
  denied: mdiClose,
  paused: mdiPauseCircleOutline,
};

interface AgentPanelProps {
  readonly agent: BrowserAgentState;
  readonly busy: boolean;
  readonly send: (command: BrowserCommand) => void;
}

export default function AgentPanel({ agent, busy, send }: AgentPanelProps) {
  const actions = [...agent.actions].reverse();
  const rights = agent.origin ? siteRights(agent.grants) : [];
  return (
    <aside className="agent-panel" aria-label={AGENT_TITLE}>
      <div className="agent-head">
        <span className={`agent-dot ${STATUS_TONES[agent.status]}`} aria-hidden="true" />
        <div className="agent-heading">
          <span className="agent-section">{AGENT_TITLE}</span>
          <strong>{STATUS_LABELS[agent.status]}</strong>
        </div>
        <IconButton
          path={mdiClose}
          label={AGENT_CLOSE}
          tooltip="bottom-right"
          onClick={() => send({ action: "agent", visible: false })}
        />
      </div>
      <div className="agent-body">
        {agent.task && (
          <section className="agent-group">
            <h2 className="agent-section">{TASK_TITLE}</h2>
            <p className="agent-task">{agent.task}</p>
          </section>
        )}
        {agent.target && (
          <div className="agent-target" role="status">
            <span>{TARGET_LABEL}</span>
            <code>{agent.target}</code>
          </div>
        )}
        <section className="agent-group agent-log-group">
          <h2 className="agent-section">{ACTIONS_TITLE}</h2>
          <ScrollArea orientation="vertical" className="agent-log">
            {actions.length === 0 ? (
              <p className="agent-empty">{NO_ACTIONS}</p>
            ) : (
              <ol>
                {actions.map((action) => {
                  const meta = actionMeta(action);
                  return (
                    <li key={action.id} className={`agent-action ${action.state}`}>
                      <span className="agent-action-mark" aria-hidden="true">
                        <Icon path={ACTION_ICONS[action.state]} size={11} />
                      </span>
                      <div>
                        <span className="agent-action-title">{actionTitle(action)}</span>
                        {meta && <small>{meta}</small>}
                      </div>
                    </li>
                  );
                })}
              </ol>
            )}
          </ScrollArea>
        </section>
        {rights.length > 0 && agent.origin && (
          <section className="agent-group">
            <h2 className="agent-section">
              {RIGHTS_TITLE} {siteName(agent.origin)}
            </h2>
            {rights.map((right) => (
              <div className="agent-right" key={right.grant.id}>
                <span>{right.label}</span>
                <IconButton
                  path={mdiLinkVariantOff}
                  label={REVOKE}
                  tooltip="top-right"
                  iconSize={14}
                  className="agent-revoke"
                  disabled={busy}
                  onClick={() => send({ action: "agentRevoke", id: right.grant.id })}
                />
              </div>
            ))}
          </section>
        )}
        {canStop(agent.status) && (
          <div className="agent-controls">
            {canResume(agent.status) ? (
              <button type="button" disabled={busy} onClick={() => send({ action: "agentResume" })}>
                <Icon path={mdiPlay} size={15} />
                {RESUME}
              </button>
            ) : (
              <button
                type="button"
                disabled={busy || !canTakeControl(agent.status)}
                onClick={() => send({ action: "agentControl" })}
              >
                <Icon path={mdiHandBackRightOutline} size={15} />
                {TAKE_CONTROL}
              </button>
            )}
            <button
              type="button"
              className="danger-soft"
              disabled={busy}
              onClick={() => send({ action: "agentStop" })}
            >
              <Icon path={mdiStop} size={15} />
              {STOP}
            </button>
          </div>
        )}
      </div>
    </aside>
  );
}
