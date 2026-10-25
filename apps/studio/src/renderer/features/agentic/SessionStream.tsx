import { useEffect, useLayoutEffect, useRef, type CSSProperties } from "react";
import { observer } from "mobx-react-lite";
import { mdiShieldAlertOutline } from "@mdi/js";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { AGENTIC_INSTRUCTION_MAX } from "@zvs/shared";
import AssistantAvatar from "../../ui/atoms/AssistantAvatar";
import Button from "../../ui/atoms/buttons/Button";
import Icon from "../../ui/atoms/Icon";
import GenerationStatus from "../../ui/molecules/GenerationStatus";
import TextComposer from "../../ui/molecules/TextComposer";
import ChatMarkdown from "../chat/ChatMarkdown";
import FileChangeList from "../chat/FileChangeList";
import { commandLine } from "../chat/processCopy";
import { toolTitle } from "../chat/toolCallCopy";
import { shouldPinToBottom } from "../chat/scroll";
import {
  APPROVAL_TITLE,
  APPROVE_ALWAYS,
  APPROVE_ONCE,
  DENY,
  EXTEND_SESSION,
  extendLead,
  GOAL_TITLE,
  INSTRUCTION_LABEL,
  INSTRUCTION_PLACEHOLDER,
  INSTRUCTION_SEND,
  INSTRUCTIONS_QUEUED,
  LOADING,
  PHASE_LABELS,
  STEP_WORKING,
  STOP_REASON_COPY,
  STREAM_TITLE,
  SUMMARY_TITLE,
} from "./agenticCopy";
import type AgenticStore from "./AgenticStore";
import type { SessionApproval } from "./sessionModel";
import ToolCard from "./ToolCard";

function Eyebrow({ children }: { readonly children: string }) {
  return (
    <p className="m-0 mb-1 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
      {children}
    </p>
  );
}

function approvalDetail(approval: SessionApproval): string | null {
  const process = approval.process;
  if (process !== null) {
    if (process.source !== undefined) return process.source;
    return commandLine(process.argv);
  }
  return approval.changes.length > 0 ? null : approval.action;
}

const ApprovalBar = observer(function ApprovalBar({ store }: { readonly store: AgenticStore }) {
  const approval = store.stream.approval;
  if (approval === null) return null;
  const detail = approvalDetail(approval);
  return (
    <div
      role="alert"
      style={{ "--zvs-beam-color": "var(--color-warn)" } as CSSProperties}
      className="zvs-beam flex flex-col gap-2.5 rounded-card border border-warn-line bg-warn-soft px-3.5 py-3"
    >
      <div className="flex flex-wrap items-center gap-3">
        <span className="text-warn">
          <Icon path={mdiShieldAlertOutline} size={18} />
        </span>
        <p className="m-0 min-w-0 flex-1 text-[13px] font-semibold text-main-50">
          {`${APPROVAL_TITLE}: «${toolTitle(approval.subject)}»`}
        </p>
        <div className="flex flex-none gap-2">
          <Button
            size="sm"
            tone="ghost"
            disabled={store.deciding}
            onClick={() => void store.decide("deny")}
          >
            {DENY}
          </Button>
          <Button size="sm" disabled={store.deciding} onClick={() => void store.decide("always")}>
            {APPROVE_ALWAYS}
          </Button>
          <Button
            size="sm"
            tone="primary"
            disabled={store.deciding}
            onClick={() => void store.decide("once")}
          >
            {APPROVE_ONCE}
          </Button>
        </div>
      </div>
      {detail === null ? null : (
        <pre className="m-0 max-h-40 overflow-auto rounded-control bg-main-900 px-3 py-2 font-mono text-[11.5px] whitespace-pre-wrap break-all text-main-200">
          {detail}
        </pre>
      )}
      {approval.changes.length === 0 ? null : (
        <div className="max-h-80 overflow-auto">
          <FileChangeList changes={approval.changes} expanded />
        </div>
      )}
    </div>
  );
});

const Outcome = observer(function Outcome({ store }: { readonly store: AgenticStore }) {
  const session = store.session;
  const phase = store.phase;
  if (session === null || phase === null) return null;
  if (phase === "completed") {
    return (
      <div className="rounded-card border border-ok-line bg-ok-soft px-4 py-3">
        <Eyebrow>{SUMMARY_TITLE}</Eyebrow>
        {session.summary === null ? (
          <p className="m-0 text-[13px] text-main-100">{STOP_REASON_COPY.goal}</p>
        ) : (
          <ChatMarkdown content={session.summary} />
        )}
      </div>
    );
  }
  if (phase === "budget") {
    const budget = store.extension;
    return (
      <div className="flex flex-wrap items-center gap-3 rounded-card border border-warn-line bg-warn-soft px-4 py-3">
        <div className="min-w-0 flex-1">
          <p className="m-0 text-[13px] font-semibold text-main-50">{PHASE_LABELS.budget}</p>
          <p className="m-0 mt-0.5 text-[12px] text-main-300">
            {session.stopReason === null ? null : `${STOP_REASON_COPY[session.stopReason]} `}
            {budget === null
              ? null
              : extendLead(budget.maxSteps, budget.maxTokens, budget.maxMinutes)}
          </p>
        </div>
        <Button tone="primary" disabled={store.extending} onClick={() => void store.extend()}>
          {EXTEND_SESSION}
        </Button>
      </div>
    );
  }
  if (phase === "failed") {
    return (
      <div className="rounded-card border border-err-border bg-err-soft px-4 py-3 text-[13px] text-err">
        {session.error ?? STOP_REASON_COPY.error}
      </div>
    );
  }
  if (phase === "stopped" || phase === "cancelled") {
    const reason = session.stopReason ?? (phase === "cancelled" ? "cancelled" : "loop");
    return (
      <div className="rounded-card border border-line bg-main-850 px-4 py-3 text-[13px] text-main-300">
        {STOP_REASON_COPY[reason]}
      </div>
    );
  }
  const working =
    store.activeCall === null && store.stream.open === null && store.stream.approval === null;
  return working ? (
    <GenerationStatus label={phase === "planning" ? PHASE_LABELS.planning : STEP_WORKING} />
  ) : null;
});

const Composer = observer(function Composer({ store }: { readonly store: AgenticStore }) {
  const session = store.session;
  if (session === null || !store.editable) return null;
  return (
    <div className="flex flex-col gap-2">
      {session.pendingInstructions.length === 0 ? null : (
        <div className="rounded-card bg-main-850 px-3 py-2">
          <Eyebrow>{INSTRUCTIONS_QUEUED}</Eyebrow>
          <ul className="m-0 flex list-none flex-col gap-1 p-0">
            {session.pendingInstructions.map((text, index) => (
              <li key={index} className="truncate text-[12px] text-main-300" title={text}>
                {text}
              </li>
            ))}
          </ul>
        </div>
      )}
      <TextComposer
        label={INSTRUCTION_LABEL}
        placeholder={INSTRUCTION_PLACEHOLDER}
        value={store.instruction}
        max={AGENTIC_INSTRUCTION_MAX}
        submitLabel={INSTRUCTION_SEND}
        canSubmit={store.instruction.trim().length > 0 && !store.instructing}
        minRows={1}
        maxRows={6}
        onChange={store.setInstruction}
        onSubmit={() => void store.instruct()}
      />
    </div>
  );
});

export default observer(function SessionStream({ store }: { readonly store: AgenticStore }) {
  const viewport = useRef<HTMLDivElement>(null);
  const content = useRef<HTMLDivElement>(null);
  const pinned = useRef(true);
  const session = store.session;
  const entries = store.stream.entries;
  const scrollLatest = () => {
    const element = viewport.current;
    if (element) element.scrollTop = element.scrollHeight;
  };
  useLayoutEffect(() => {
    pinned.current = true;
    scrollLatest();
  }, [session?.id, store.streamLoaded]);
  useLayoutEffect(() => {
    if (pinned.current) scrollLatest();
  }, [entries, store.stream.approval]);
  useEffect(() => {
    if (!content.current || typeof ResizeObserver === "undefined") return;
    const resize = new ResizeObserver(() => {
      if (pinned.current) scrollLatest();
    });
    resize.observe(content.current);
    return () => resize.disconnect();
  }, []);
  if (session === null) return null;
  const live = session.status === "running";
  const lastText = entries.at(-1)?.id;
  return (
    <section aria-label={STREAM_TITLE} className="flex min-h-0 min-w-0 flex-1 flex-col">
      <ScrollArea
        ref={viewport}
        onScroll={(event) => {
          const element = event.currentTarget;
          pinned.current = shouldPinToBottom(
            element.scrollTop,
            element.clientHeight,
            element.scrollHeight,
          );
        }}
        className="min-h-0 flex-1 px-6 pt-5 [overflow-anchor:none]"
      >
        <div ref={content} className="mx-auto flex w-full max-w-4xl flex-col gap-3.5 pb-5">
          <div className="rounded-card border border-line bg-main-850 px-4 py-3">
            <Eyebrow>{GOAL_TITLE}</Eyebrow>
            <p className="m-0 text-[13px] leading-relaxed whitespace-pre-wrap text-main-100">
              {session.goal}
            </p>
          </div>
          {!store.streamLoaded ? (
            <p role="status" className="m-0 text-[12px] text-main-500">
              {LOADING}
            </p>
          ) : null}
          {entries.map((entry) =>
            entry.kind === "tool" ? (
              <ToolCard key={entry.id} step={entry.step} />
            ) : entry.text.trim().length === 0 ? null : (
              <div key={entry.id} className="flex gap-3">
                <AssistantAvatar
                  size={24}
                  live={live && entry.id === lastText && store.stream.open === entry.id}
                  className="mt-0.5"
                />
                <ChatMarkdown
                  content={entry.text}
                  streaming={live && store.stream.open === entry.id}
                />
              </div>
            ),
          )}
          {store.streamLoaded ? <Outcome store={store} /> : null}
        </div>
      </ScrollArea>
      <div className="flex-none px-6 pb-5">
        <div className="mx-auto flex w-full max-w-4xl flex-col gap-2.5">
          <ApprovalBar store={store} />
          <Composer store={store} />
        </div>
      </div>
    </section>
  );
});
