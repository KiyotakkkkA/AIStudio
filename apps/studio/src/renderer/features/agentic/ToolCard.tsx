import { useState } from "react";
import { mdiChevronDown } from "@mdi/js";
import Icon from "../../ui/atoms/Icon";
import StatusDot from "../../ui/atoms/StatusDot";
import { STATUS_TONE_TEXT } from "../../ui/atoms/statusTone";
import FileChangeList from "../chat/FileChangeList";
import { CATEGORY_BADGE, toolPresentation } from "../chat/toolCallCopy";
import type { RunStep } from "../scenarios/runModel";
import { durationLabel } from "../scenarios/runCopy";
import { linesHidden, NO_OUTPUT, SHOW_LESS, STDERR_TITLE } from "./agenticCopy";
import {
  cardBody,
  cardResult,
  cardTarget,
  changeTotals,
  opensByDefault,
  outputPreview,
  type CardBody,
} from "./toolCardModel";

function OutputBlock({
  text,
  tone = "text-main-300",
}: {
  readonly text: string;
  readonly tone?: string;
}) {
  const [full, setFull] = useState(false);
  const preview = outputPreview(text);
  return (
    <div>
      <pre
        className={`m-0 px-3 py-2 font-mono text-[11.5px] leading-[1.6] whitespace-pre-wrap break-all ${tone}`}
      >
        {full ? text.replace(/\n+$/, "") : preview.shown}
      </pre>
      {preview.hidden > 0 ? (
        <button
          type="button"
          className="px-3 pb-2 text-[11.5px] text-accent-medium hover:text-accent-light"
          onClick={() => {
            setFull(!full);
          }}
        >
          {full ? SHOW_LESS : linesHidden(preview.hidden)}
        </button>
      ) : null}
    </div>
  );
}

function Body({ body }: { readonly body: CardBody }) {
  switch (body.kind) {
    case "changes":
      return (
        <div className="p-1.5">
          <FileChangeList changes={body.changes} expanded />
        </div>
      );
    case "process": {
      const empty = body.stdout.trim().length === 0 && body.stderr.trim().length === 0;
      return (
        <div className="py-1">
          {empty ? <p className="m-0 px-3 py-2 text-[11.5px] text-main-500">{NO_OUTPUT}</p> : null}
          {body.stdout.trim().length > 0 ? <OutputBlock text={body.stdout} /> : null}
          {body.stderr.trim().length > 0 ? (
            <div>
              <p className="m-0 px-3 pt-1 font-mono text-[10.5px] text-main-500">{STDERR_TITLE}</p>
              <OutputBlock text={body.stderr} tone="text-err" />
            </div>
          ) : null}
        </div>
      );
    }
    case "text":
      return <OutputBlock text={body.text} />;
    case "error":
      return <p className="m-0 px-3 py-2 text-[12px] text-err">{body.text}</p>;
  }
}

export default function ToolCard({ step }: { readonly step: RunStep }) {
  const [open, setOpen] = useState(() => opensByDefault(step));
  const presentation = toolPresentation(step.type);
  const body = cardBody(step);
  const result = cardResult(step);
  const totals = body?.kind === "changes" ? changeTotals(body.changes) : null;
  const duration = step.finishedAt === null ? null : step.finishedAt - step.startedAt;
  const expandable = body !== null;
  return (
    <div className="overflow-hidden rounded-card border border-line bg-main-850">
      <button
        type="button"
        aria-expanded={expandable ? open : undefined}
        disabled={!expandable}
        className="flex w-full items-center gap-2 px-3 py-2 text-left enabled:cursor-pointer"
        onClick={() => {
          setOpen(!open);
        }}
      >
        <span
          className={`inline-flex h-5 flex-none items-center gap-1 rounded-[5px] px-1.75 text-[10.5px] font-medium ${CATEGORY_BADGE[presentation.category]}`}
        >
          <Icon path={presentation.icon} size={12} />
          {presentation.title}
        </span>
        <span
          className="min-w-0 flex-1 truncate font-mono text-[11.5px] text-main-200"
          title={cardTarget(step)}
        >
          {cardTarget(step)}
        </span>
        {totals === null ? null : (
          <>
            <span className="flex-none font-mono text-[10.5px] text-ok tabular-nums">
              +{totals.added.toLocaleString("ru-RU")}
            </span>
            <span className="flex-none font-mono text-[10.5px] text-err tabular-nums">
              −{totals.removed.toLocaleString("ru-RU")}
            </span>
          </>
        )}
        {result === null ? null : (
          <span
            className={`inline-flex h-5 flex-none items-center gap-1.5 rounded-full bg-main-700 px-2 text-[10.5px] font-medium ${STATUS_TONE_TEXT[result.tone]}`}
          >
            <StatusDot tone={result.tone} />
            {result.label}
          </span>
        )}
        {duration === null ? null : (
          <span className="flex-none font-mono text-[10.5px] text-main-500 tabular-nums">
            {durationLabel(duration)}
          </span>
        )}
        {expandable ? (
          <Icon
            path={mdiChevronDown}
            size={15}
            className={`flex-none text-main-500 transition-transform ${open ? "rotate-180" : ""}`}
          />
        ) : null}
      </button>
      {open && body !== null ? (
        <div className="border-t border-line">
          <Body body={body} />
        </div>
      ) : null}
    </div>
  );
}
