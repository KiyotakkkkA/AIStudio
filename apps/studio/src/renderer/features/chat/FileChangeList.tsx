import { useState } from "react";
import type { FileDiffDto } from "@zvs/shared";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import Button from "../../ui/atoms/buttons/Button";
import {
  CHANGE_KIND_LABEL,
  DIFF_TRUNCATED,
  NO_TEXT_CHANGES,
  VISIBLE_CHANGES,
  diffLineTone,
  diffLines,
  moreChanges,
  type DiffLineTone,
} from "./fileChangesCopy";

const LINE_TONE: Record<DiffLineTone, string> = {
  added: "bg-ok-soft text-ok",
  removed: "bg-err-soft text-err",
  hunk: "text-main-500",
  context: "text-main-300",
};

function FileChange({ change, open }: { readonly change: FileDiffDto; readonly open: boolean }) {
  const lines = diffLines(change.diff);
  return (
    <li className="overflow-hidden rounded-control bg-main-900">
      <details open={open} className="group/file">
        <summary className="flex min-h-8 cursor-pointer list-none items-center gap-2 px-2.5 py-1.5 text-[11.5px] [&::-webkit-details-marker]:hidden">
          <span className="shrink-0 text-main-400">{CHANGE_KIND_LABEL[change.kind]}</span>
          <span
            className="min-w-0 flex-1 truncate font-mono text-main-100"
            title={change.from === undefined ? change.path : `${change.from} → ${change.path}`}
          >
            {change.from === undefined ? change.path : `${change.from} → ${change.path}`}
          </span>
          <span className="shrink-0 font-mono text-ok tabular-nums">+{change.added}</span>
          <span className="shrink-0 font-mono text-err tabular-nums">−{change.removed}</span>
        </summary>
        {lines.length === 0 ? (
          <p className="m-0 px-2.5 pb-2 text-[11.5px] text-main-500">{NO_TEXT_CHANGES}</p>
        ) : (
          <ScrollArea className="max-h-72 px-1 pb-1.5">
            <pre className="m-0 font-mono text-[11.5px] leading-[1.5]">
              {lines.map((line, index) => (
                <span
                  key={index}
                  className={`block px-1.5 whitespace-pre-wrap ${LINE_TONE[diffLineTone(line)]}`}
                >
                  {line.length === 0 ? " " : line}
                </span>
              ))}
            </pre>
            {change.truncated && (
              <p className="m-0 px-1.5 pt-1 text-[11px] text-main-500">{DIFF_TRUNCATED}</p>
            )}
          </ScrollArea>
        )}
      </details>
    </li>
  );
}

export default function FileChangeList({
  changes,
  expanded = false,
}: {
  readonly changes: readonly FileDiffDto[];
  readonly expanded?: boolean;
}) {
  const [all, setAll] = useState(false);
  const shown = all ? changes : changes.slice(0, VISIBLE_CHANGES);
  const hidden = changes.length - shown.length;
  return (
    <div className="space-y-1.5">
      <ul className="space-y-1.5">
        {shown.map((change) => (
          <FileChange
            key={`${change.from ?? ""}>${change.path}`}
            change={change}
            open={expanded && changes.length <= VISIBLE_CHANGES}
          />
        ))}
      </ul>
      {hidden > 0 && (
        <Button
          onClick={() => {
            setAll(true);
          }}
        >
          {moreChanges(hidden)}
        </Button>
      )}
    </div>
  );
}
