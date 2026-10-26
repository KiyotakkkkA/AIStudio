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
  NO_SITE,
  NO_TASK,
  RESUME,
  REVOKE,
  RIGHTS_TITLE,
  RIGHT_TIER_LABELS,
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
  paceLabel,
  siteName,
  siteRights,
  stepLabel,
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
  const tone = STATUS_TONES[agent.status];
  const actions = [...agent.actions].reverse();
  return (
    <aside className="agent-panel" aria-label={AGENT_TITLE}>
      <div className="agent-head">
        <span className="agent-title">{AGENT_TITLE}</span>
        <IconButton
          path={mdiClose}
          label={AGENT_CLOSE}
          tooltip="bottom-right"
          onClick={() => send({ action: "agent", visible: false })}
        />
      </div>
      <div className="agent-body">
        <div className="agent-card agent-state">
          <span className={`agent-dot ${tone}`} aria-hidden="true" />
          <div>
            <strong>{STATUS_LABELS[agent.status]}</strong>
            {agent.status !== "idle" && (
              <small>
                {stepLabel(agent.steps)} · {paceLabel(agent.pace, agent.limit)}
              </small>
            )}
          </div>
        </div>
        <section>
          <h2 className="agent-section">{TASK_TITLE}</h2>
          <p className="agent-card agent-task">{agent.task ?? NO_TASK}</p>
        </section>
        {agent.target && (
          <div className="agent-target" role="status">
            <span>{TARGET_LABEL}</span>
            <code>{agent.target}</code>
          </div>
        )}
        <h2 className="agent-section">{ACTIONS_TITLE}</h2>
        <ScrollArea orientation="vertical" className="agent-log">
          {actions.length === 0 ? (
            <p className="agent-empty">{NO_ACTIONS}</p>
          ) : (
            <ol>
              {actions.map((action) => (
                <li key={action.id} className={`agent-action ${action.state}`}>
                  <span className="agent-action-mark" aria-hidden="true">
                    <Icon path={ACTION_ICONS[action.state]} size={11} />
                  </span>
                  <div>
                    <code>{actionTitle(action)}</code>
                    <small>{actionMeta(action)}</small>
                  </div>
                </li>
              ))}
            </ol>
          )}
        </ScrollArea>
        <div className="agent-card agent-rights">
          <h2 className="agent-section">{RIGHTS_TITLE}</h2>
          {agent.origin ? (
            <>
              <span className="agent-site">{siteName(agent.origin)}</span>
              {siteRights(agent.grants).map((right) => (
                <div className="agent-right" key={right.label}>
                  <span>{right.label}</span>
                  <span className={`agent-tier ${right.tier}`}>
                    {RIGHT_TIER_LABELS[right.tier]}
                  </span>
                  {right.grant ? (
                    <IconButton
                      path={mdiLinkVariantOff}
                      label={REVOKE}
                      tooltip="top-right"
                      iconSize={14}
                      className="agent-revoke"
                      disabled={busy}
                      onClick={() => send({ action: "agentRevoke", id: right.grant!.id })}
                    />
                  ) : null}
                </div>
              ))}
            </>
          ) : (
            <p className="agent-empty">{NO_SITE}</p>
          )}
        </div>
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
            disabled={busy || !canStop(agent.status)}
            onClick={() => send({ action: "agentStop" })}
          >
            <Icon path={mdiStop} size={15} />
            {STOP}
          </button>
        </div>
      </div>
    </aside>
  );
}
