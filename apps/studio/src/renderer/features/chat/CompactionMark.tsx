import { useState } from "react";
import { mdiArrowCollapseVertical } from "@mdi/js";
import type { ChatCompactionDto } from "@zvs/shared";
import Icon from "../../ui/atoms/Icon";
import {
  COMPACTION_AUTO,
  COMPACTION_HIDE,
  COMPACTION_SHOW,
  COMPACTION_TITLE,
  COMPACTION_UNDO,
  compactionMessages,
  compactionTokens,
} from "./compactionCopy";

export default function CompactionMark({
  compaction,
  latest,
  disabled,
  onUndo,
}: {
  readonly compaction: ChatCompactionDto;
  readonly latest: boolean;
  readonly disabled: boolean;
  readonly onUndo: () => void;
}) {
  const [open, setOpen] = useState(false);
  const details = [
    compactionMessages(compaction.messages),
    compactionTokens(compaction.tokensBefore, compaction.tokensAfter),
    ...(compaction.trigger === "auto" ? [COMPACTION_AUTO] : []),
  ].join(" · ");
  return (
    <section aria-label={COMPACTION_TITLE} className="flex flex-col gap-2">
      <div className="flex items-center gap-3 text-[12px] text-main-400">
        <span aria-hidden="true" className="h-px flex-1 bg-line" />
        <span className="inline-flex min-w-0 items-center gap-1.5">
          <Icon path={mdiArrowCollapseVertical} size={13} />
          <span className="font-medium text-main-200">{COMPACTION_TITLE}</span>
          <span className="truncate">{details}</span>
        </span>
        <span aria-hidden="true" className="h-px flex-1 bg-line" />
      </div>
      <div className="flex justify-center gap-1">
        <button
          type="button"
          aria-expanded={open}
          onClick={() => setOpen((value) => !value)}
          className="rounded-control px-2 py-0.5 text-[12px] text-accent-medium hover:bg-main-750"
        >
          {open ? COMPACTION_HIDE : COMPACTION_SHOW}
        </button>
        {latest ? (
          <button
            type="button"
            disabled={disabled}
            onClick={onUndo}
            className="rounded-control px-2 py-0.5 text-[12px] text-main-300 hover:bg-main-750 disabled:opacity-50"
          >
            {COMPACTION_UNDO}
          </button>
        ) : null}
      </div>
      {open ? (
        <p className="m-0 rounded-card bg-main-850 px-3.5 py-2.5 text-[12.5px] leading-relaxed text-main-200 wrap-break-word whitespace-pre-wrap">
          {compaction.summary}
        </p>
      ) : null}
    </section>
  );
}
