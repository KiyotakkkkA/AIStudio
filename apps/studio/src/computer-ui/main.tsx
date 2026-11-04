import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { createRoot } from "react-dom/client";
import { mdiChevronDown, mdiChevronUp, mdiSendOutline, mdiStop } from "@mdi/js";
import {
  FolderApprovalDto,
  type ApprovalRequestDto,
  type ComputerCommand,
  type ComputerPointer,
  type ComputerState,
  type FolderAccessAnswer,
} from "@zvs/shared";
import "../renderer/app/theme.css";
import "./style.css";
import Icon from "../browser-ui/Icon";
import { applyChromeAppearance } from "../browser-ui/appearance";
import AnswerFlow from "../renderer/features/chat/AnswerFlow";
import ChatMarkdown from "../renderer/features/chat/ChatMarkdown";
import { ToolCallItems, type ToolCallApproval } from "../renderer/features/chat/ChatToolCalls";
import { ToolCallSlotsContext, type ToolCallSlots } from "../renderer/features/chat/ToolCallSlots";
import { approvalView } from "../renderer/features/chat/approvalModel";
import { ALLOW_ONCE, FORBID, approvalQuestion } from "../renderer/features/chat/toolCallCopy";
import {
  ALLOW_ALWAYS,
  ALLOW_CONVERSATION,
  ALLOW_ONCE as ALLOW_FOLDER_ONCE,
  DENY,
  accessLead,
  accessTitle,
} from "../renderer/features/chat/workingFolderCopy";
import { IN_BACKGROUND } from "../renderer/features/backgroundShells/backgroundShellsCopy";
import MailDraftView from "../renderer/features/mail/MailDraftView";
import { mailPreview } from "../renderer/features/mail/mailCopy";
import Button from "../renderer/ui/atoms/buttons/Button";
import GenerationStatus from "../renderer/ui/molecules/GenerationStatus";
import { GLOW_LABEL, sendCommand, subscribePointer, subscribeState, windowLabel } from "./bridge";
import {
  BADGE_TEXT,
  COLLAPSE,
  COMMAND_FAILED,
  COMPOSER_PLACEHOLDER,
  ELAPSED_TITLE,
  EXPAND,
  FEED_EMPTY,
  FEED_TITLE,
  PANEL_TITLE,
  SEND,
  STATUS_TEXT,
  STOP,
  STOP_HINT,
  elapsedText,
  statusLine,
} from "./computerCopy";

const DARK_SCHEME_QUERY = "(prefers-color-scheme: dark)";
const STICK_DISTANCE = 48;

const INITIAL: ComputerState = {
  active: false,
  status: "idle",
  apps: [],
  actions: [],
  collapsed: false,
  appearance: { theme: "system", darkPalette: "zvs-dark", lightPalette: "zvs-light" },
  feed: { content: "", reasoning: "", toolCalls: [], approvals: [] },
};

const PANEL_SLOTS: ToolCallSlots = {
  images: () => null,
  shell: () => <p className="m-0 font-mono text-[11.5px] text-main-300">{IN_BACKGROUND}</p>,
};

type Run = (command: ComputerCommand) => Promise<boolean>;

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
  useEffect(
    () =>
      subscribeState((next) =>
        setState({ ...INITIAL, ...next, feed: { ...INITIAL.feed, ...next.feed } }),
      ),
    [],
  );
  useEffect(() => applyChromeAppearance(state.appearance, osDark), [state.appearance, osDark]);
  return state;
}

function useNow(ticking: boolean): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!ticking) return;
    setNow(Date.now());
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [ticking]);
  return now;
}

function approvalKey(request: ApprovalRequestDto): string | null {
  return typeof request.id === "string" ? request.id : null;
}

function FolderRequest({
  request,
  busy,
  onAnswer,
}: {
  readonly request: FolderApprovalDto;
  readonly busy: boolean;
  readonly onAnswer: (answer: FolderAccessAnswer) => void;
}) {
  const answers: readonly { answer: FolderAccessAnswer; label: string }[] = [
    { answer: "once", label: ALLOW_FOLDER_ONCE },
    { answer: "conversation", label: ALLOW_CONVERSATION },
    { answer: "always", label: ALLOW_ALWAYS },
    { answer: "deny", label: DENY },
  ];
  return (
    <div role="status" className="space-y-2 rounded-control bg-warn-soft px-3 py-2.5">
      <p className="m-0 text-[12.5px] font-medium text-main-100">{accessTitle(request)}</p>
      <p className="m-0 text-[12.5px] text-main-200">{accessLead(request)}</p>
      <ul className="m-0 list-none space-y-1 p-0">
        {request.folders.map((folder) => (
          <li
            key={folder.path}
            className="rounded-control bg-main-850 px-2.5 py-1.5 font-mono text-[12px] break-all text-main-100"
          >
            {folder.path}
          </li>
        ))}
      </ul>
      <div className="flex flex-wrap gap-2">
        {answers.map(({ answer, label }) => (
          <Button
            key={answer}
            size="sm"
            disabled={busy}
            {...(answer === "once" ? { tone: "primary" as const } : {})}
            onClick={() => onAnswer(answer)}
          >
            {label}
          </Button>
        ))}
      </div>
    </div>
  );
}

function PlainRequest({
  request,
  busy,
  onDecide,
}: {
  readonly request: ApprovalRequestDto;
  readonly busy: boolean;
  readonly onDecide: (approve: boolean) => void;
}) {
  const subject = typeof request.subject === "string" ? request.subject : "";
  const mail = mailPreview(request);
  return (
    <div
      role="status"
      className="flex flex-wrap items-center gap-3 rounded-control bg-warn-soft px-3 py-2.5"
    >
      <p className="m-0 min-w-0 flex-1 text-[12.5px] text-main-100">{approvalQuestion(subject)}</p>
      {mail === null ? null : (
        <div className="w-full">
          <MailDraftView preview={mail} expanded />
        </div>
      )}
      <div className="flex gap-2">
        <Button size="sm" disabled={busy} tone="primary" onClick={() => onDecide(true)}>
          {ALLOW_ONCE}
        </Button>
        <Button size="sm" disabled={busy} onClick={() => onDecide(false)}>
          {FORBID}
        </Button>
      </div>
    </div>
  );
}

function Feed({
  state,
  live,
  run,
}: {
  readonly state: ComputerState;
  readonly live: boolean;
  readonly run: Run;
}) {
  const { feed } = state;
  const [busy, setBusy] = useState<readonly string[]>([]);
  const send = (id: string, command: ComputerCommand) => {
    setBusy((current) => [...current, id]);
    void run(command).then((done) => {
      if (!done) setBusy((current) => current.filter((item) => item !== id));
    });
  };
  const decide = (id: string, approve: boolean) =>
    send(id, approve ? { action: "approve", id } : { action: "deny", id });
  const answer = (id: string, value: FolderAccessAnswer) =>
    send(id, { action: "answer", id, answer: value });
  const views = feed.approvals.flatMap((request) => {
    const view = approvalView(request);
    return view === null ? [] : [view];
  });
  const approvalFor = (id: string | null): ToolCallApproval | undefined => {
    const view = id === null ? undefined : views.find((item) => item.id === id);
    return view === undefined ? undefined : { ...view, busy: busy.includes(view.id) };
  };
  const attached = new Set(
    feed.toolCalls.flatMap((call) => (call.approvalId === null ? [] : [call.approvalId])),
  );
  const standalone = feed.approvals.filter((request) => {
    const id = approvalKey(request);
    return id !== null && !attached.has(id);
  });
  const empty =
    feed.content.trim() === "" &&
    feed.reasoning.trim() === "" &&
    feed.toolCalls.length === 0 &&
    standalone.length === 0;
  const requests: ReactNode[] = standalone.map((request) => {
    const id = approvalKey(request) ?? "";
    const folder = FolderApprovalDto.safeParse(request);
    return folder.success ? (
      <FolderRequest
        key={id}
        request={folder.data}
        busy={busy.includes(id)}
        onAnswer={(value) => answer(id, value)}
      />
    ) : (
      <PlainRequest
        key={id}
        request={request}
        busy={busy.includes(id)}
        onDecide={(approve) => decide(id, approve)}
      />
    );
  });
  return (
    <ToolCallSlotsContext.Provider value={PANEL_SLOTS}>
      <section aria-label={FEED_TITLE} className="space-y-3">
        {empty && !live ? <p className="feed-empty">{FEED_EMPTY}</p> : null}
        <AnswerFlow
          content={feed.content}
          reasoning={feed.reasoning}
          rounds={feed.rounds ?? []}
          toolCalls={feed.toolCalls}
          generating={live}
          renderToolItems={(calls) => (
            <ToolCallItems
              calls={calls}
              approvalFor={approvalFor}
              onDecide={decide}
              onAnswerAccess={answer}
              questionFor={() => undefined}
            />
          )}
          renderText={(text, last) => <ChatMarkdown content={text} streaming={last} />}
        />
        {requests}
        {live ? (
          <GenerationStatus
            label={statusLine(state.status, state.actions)}
            since={state.startedAt ?? null}
          />
        ) : null}
      </section>
    </ToolCallSlotsContext.Provider>
  );
}

function useStickToBottom(change: unknown) {
  const box = useRef<HTMLDivElement>(null);
  const pinned = useRef(true);
  useLayoutEffect(() => {
    const element = box.current;
    if (element !== null && pinned.current) element.scrollTop = element.scrollHeight;
  }, [change]);
  const onScroll = () => {
    const element = box.current;
    if (element === null) return;
    pinned.current =
      element.scrollHeight - element.scrollTop - element.clientHeight < STICK_DISTANCE;
  };
  return { box, onScroll };
}

function Composer({ run }: { readonly run: Run }) {
  const [text, setText] = useState("");
  const field = useRef<HTMLTextAreaElement>(null);
  const submit = () => {
    const value = text.trim();
    if (value.length === 0) return;
    setText("");
    field.current?.blur();
    void run({ action: "say", text: value });
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
        onPointerDown={() => void run({ action: "engage" })}
        onFocus={() => void run({ action: "engage" })}
        onBlur={() => void run({ action: "release" })}
        onChange={(event) => setText(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter" && !event.shiftKey) {
            event.preventDefault();
            submit();
          }
          if (event.key === "Escape") field.current?.blur();
        }}
      />
      <button
        type="submit"
        className="chrome-button icon-button"
        aria-label={SEND}
        disabled={text.trim() === ""}
      >
        <Icon path={mdiSendOutline} size={16} />
      </button>
    </form>
  );
}

function Panel({ state }: { readonly state: ComputerState }) {
  const [error, setError] = useState<string | null>(null);
  const live = state.active && state.status !== "idle" && state.status !== "stopping";
  const now = useNow(live && state.collapsed && state.startedAt !== undefined);
  const { box, onScroll } = useStickToBottom(state.feed);
  const run: Run = (command) => {
    setError(null);
    return sendCommand(command).then(
      () => true,
      (reason: unknown) => {
        setError(typeof reason === "string" ? reason : COMMAND_FAILED);
        return false;
      },
    );
  };
  const elapsed = state.collapsed ? elapsedText(state.startedAt, now) : null;
  return (
    <main className={`panel ${state.collapsed ? "collapsed" : ""}`}>
      <header className="panel-header">
        <span className={`pulse status-${state.status}`} aria-hidden="true" />
        <span className="panel-heading">
          <span className="panel-title">{PANEL_TITLE}</span>
          <span className="panel-subtitle">{state.title ?? STATUS_TEXT[state.status]}</span>
        </span>
        {elapsed === null ? null : (
          <span className="elapsed" title={ELAPSED_TITLE}>
            {elapsed}
          </span>
        )}
        <button
          type="button"
          className="chrome-button icon-button"
          aria-label={state.collapsed ? EXPAND : COLLAPSE}
          onClick={() => void run({ action: "collapse", collapsed: !state.collapsed })}
        >
          <Icon path={state.collapsed ? mdiChevronUp : mdiChevronDown} size={16} />
        </button>
        <button
          type="button"
          className="chrome-button danger"
          onClick={() => void run({ action: "stop" })}
        >
          <Icon path={mdiStop} size={14} />
          {STOP}
        </button>
      </header>
      {state.collapsed ? null : (
        <>
          <p className="hint">{STOP_HINT}</p>
          <div ref={box} className="panel-body" onScroll={onScroll}>
            <Feed state={state} live={live} run={run} />
          </div>
          {error === null ? null : <p className="panel-error">{error}</p>}
          <Composer run={run} />
        </>
      )}
    </main>
  );
}

function usePointer(): ComputerPointer | null {
  const [pointer, setPointer] = useState<ComputerPointer | null>(null);
  useEffect(() => subscribePointer(setPointer), []);
  return pointer;
}

function AgentPointer() {
  const pointer = usePointer();
  const [clicks, setClicks] = useState(0);
  const pressed = useRef(false);
  useEffect(() => {
    if (pointer === null) return;
    if (pointer.down && !pressed.current) setClicks((count) => count + 1);
    pressed.current = pointer.down;
  }, [pointer]);
  if (pointer === null) return null;
  const scale = window.devicePixelRatio || 1;
  const x = pointer.x / scale;
  const y = pointer.y / scale;
  if (x < 0 || y < 0 || x > window.innerWidth || y > window.innerHeight) return null;
  return (
    <div
      className={`agent-pointer ${pointer.down ? "agent-pointer-down" : ""}`}
      style={{ transform: `translate(${x}px, ${y}px)` }}
      aria-hidden="true"
    >
      {clicks > 0 ? <span key={clicks} className="agent-ripple" /> : null}
      <svg className="agent-arrow" width="26" height="26" viewBox="0 0 24 24">
        <path d="M4 2.5 19.5 12 12.6 13.4 16.4 20.8 13.5 22.2 9.7 14.8 4.6 19.5Z" />
      </svg>
    </div>
  );
}

function Glow({ state }: { readonly state: ComputerState }) {
  return (
    <div className={`glow status-${state.status}`}>
      <div className="badge" role="status">
        <span className={`pulse status-${state.status}`} aria-hidden="true" />
        <span className="badge-text">{BADGE_TEXT}</span>
        <span className="badge-status">{statusLine(state.status, state.actions)}</span>
        <kbd>{STOP_HINT}</kbd>
      </div>
      <AgentPointer />
    </div>
  );
}

function ComputerApp() {
  const state = useComputerState();
  return windowLabel() === GLOW_LABEL ? <Glow state={state} /> : <Panel state={state} />;
}

createRoot(document.getElementById("root")!).render(<ComputerApp />);
