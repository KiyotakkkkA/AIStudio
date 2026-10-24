import { useEffect, useId, useState, type ReactNode } from "react";
import { motion } from "motion/react";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import {
  mdiCheck,
  mdiChevronDown,
  mdiContentCopy,
  mdiFileOutline,
  mdiFolderOutline,
  mdiMagnify,
  mdiPuzzleOutline,
} from "@mdi/js";
import type { ChatCitationDto } from "@zvs/shared";
import Icon from "../../ui/atoms/Icon";
import IconButton from "../../ui/atoms/buttons/IconButton";
import Collapse from "../../ui/motion/Collapse";
import { useFx, useTransition } from "../../ui/motion/fx";
import { plural } from "../connections/connectionPresentation";
import { sourceName } from "./citations";
import { commandLine } from "./processCopy";
import {
  ALL_STORES,
  COPY_RESULT,
  DOCUMENT_FIELD,
  FILES_TITLE,
  FILTER_FIELD,
  FOLDER_FIELD,
  FOUND_TITLE,
  IGNORE_CASE_FLAG,
  LIMIT_FIELD,
  NOTHING_FOUND,
  RANGE_FIELD,
  RAW_ARGUMENTS,
  RECURSIVE_FLAG,
  REGEX_FLAG,
  RESULT_COPIED,
  RESULT_SHORTENED,
  RESULT_TITLE,
  SECTION_FIELD,
  SKILL_FIELD,
  STDIN_FIELD,
  STORE_FIELD,
  WORKING_FOLDER,
  lineRange,
  scorePercent,
} from "./toolCallCopy";
import { resultLines, type ToolCallView } from "./toolCallView";

export function Eyebrow({ children }: { readonly children: ReactNode }) {
  return (
    <p className="mb-1 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
      {children}
    </p>
  );
}

function Tag({
  children,
  mono = false,
}: {
  readonly children: ReactNode;
  readonly mono?: boolean;
}) {
  return (
    <span
      className={`inline-flex max-w-full items-center gap-1 truncate rounded-pill bg-main-700 px-1.5 py-0.5 text-[11px] text-main-300 ${mono ? "font-mono" : ""}`}
    >
      {children}
    </span>
  );
}

function Tags({ children }: { readonly children: ReactNode }) {
  return <div className="flex flex-wrap items-center gap-1.5">{children}</div>;
}

function Field({ label, children }: { readonly label: string; readonly children: ReactNode }) {
  return (
    <span className="inline-flex min-w-0 items-center gap-1 text-[11px] text-main-500">
      {label}
      <Tag mono>{children}</Tag>
    </span>
  );
}

function QueryLine({ query, mono = false }: { readonly query: string; readonly mono?: boolean }) {
  return (
    <div className="flex items-start gap-2 rounded-control bg-main-900 px-2.5 py-2 fx-on:shadow-[inset_0_0_0_1px_var(--color-line)]">
      <Icon path={mdiMagnify} size={14} className="mt-px shrink-0 text-main-500" />
      <span
        className={`min-w-0 flex-1 text-[12.5px] wrap-break-word text-main-100 ${mono ? "font-mono text-[12px]" : ""}`}
      >
        {query}
      </span>
    </div>
  );
}

function PathLine({ icon, path }: { readonly icon: string; readonly path: string }) {
  return (
    <div className="flex min-w-0 items-center gap-2 text-[12px]">
      <Icon path={icon} size={14} className="shrink-0 text-info" />
      <span className="min-w-0 flex-1 truncate font-mono text-main-200" title={path}>
        {path}
      </span>
    </div>
  );
}

function ScoreBar({ score }: { readonly score: number }) {
  const transition = useTransition("reveal");
  return (
    <span className="flex w-24 shrink-0 items-center gap-2">
      <span className="h-1 flex-1 overflow-hidden rounded-full bg-main-700">
        <motion.span
          initial={{ width: 0 }}
          animate={{ width: `${String(Math.round(score * 100))}%` }}
          transition={transition}
          className="block h-full rounded-full bg-accent-dark"
        />
      </span>
      <span className="w-8 text-right font-mono text-[10.5px] text-main-400 tabular-nums">
        {scorePercent(score)}
      </span>
    </span>
  );
}

function citationPlace(citation: ChatCitationDto): string {
  return [
    `фрагмент ${String(citation.chunkIndex + 1)}`,
    ...(citation.page === null ? [] : [`стр. ${String(citation.page)}`]),
    ...(citation.section === "" ? [] : [citation.section]),
  ].join(" · ");
}

function Found({ citations }: { readonly citations: readonly ChatCitationDto[] }) {
  const { motion: level } = useFx();
  if (citations.length === 0)
    return <p className="m-0 text-[12px] text-main-500">{NOTHING_FOUND}</p>;
  return (
    <div>
      <Eyebrow>
        {FOUND_TITLE} · {citations.length.toLocaleString("ru-RU")}
      </Eyebrow>
      <ol className="m-0 list-none space-y-1 p-0">
        {citations.map((citation, index) => (
          <motion.li
            key={`${citation.documentId}:${String(citation.chunkIndex)}`}
            initial={level === "full" ? { opacity: 0, x: -6 } : false}
            animate={{ opacity: 1, x: 0 }}
            transition={{ delay: Math.min(index, 8) * 0.035, duration: 0.2 }}
            className="flex min-w-0 items-center gap-2.5 rounded-control bg-main-900 px-2.5 py-1.5"
          >
            <span className="w-5 shrink-0 font-mono text-[10.5px] text-main-500 tabular-nums">
              {index + 1}
            </span>
            <span className="min-w-0 flex-1">
              <span
                className="block truncate text-[12px] font-medium text-main-100"
                title={citation.sourcePath}
              >
                {sourceName(citation.sourcePath)}
              </span>
              <span className="block truncate text-[11px] text-main-500">
                {citationPlace(citation)}
              </span>
            </span>
            {citation.related === null ? <ScoreBar score={citation.score} /> : null}
          </motion.li>
        ))}
      </ol>
    </div>
  );
}

function CodeBlock({ source }: { readonly source: string }) {
  const lines = source.split("\n");
  return (
    <ScrollArea className="max-h-64 rounded-control bg-main-900 py-2">
      <pre className="m-0 font-mono text-[11.5px] leading-[1.55] text-main-200">
        {lines.map((line, index) => (
          <span key={index} className="flex">
            <span
              aria-hidden="true"
              className="w-9 shrink-0 pr-2.5 text-right text-main-500 tabular-nums select-none"
            >
              {index + 1}
            </span>
            <span className="min-w-0 flex-1 pr-2.5 wrap-break-word whitespace-pre-wrap">
              {line || " "}
            </span>
          </span>
        ))}
      </pre>
    </ScrollArea>
  );
}

function ViewBody({ view }: { readonly view: ToolCallView }) {
  switch (view.kind) {
    case "search":
      return (
        <div className="space-y-2">
          <QueryLine query={view.query} />
          <Tags>
            <Field label={STORE_FIELD}>{view.store ?? ALL_STORES}</Field>
            {view.limit === null ? null : (
              <Field label={LIMIT_FIELD}>{view.limit.toLocaleString("ru-RU")}</Field>
            )}
          </Tags>
          {view.citations === null ? null : <Found citations={view.citations} />}
        </div>
      );
    case "document":
      return (
        <div className="space-y-2">
          <div className="flex min-w-0 items-center gap-2 text-[12px]">
            <span className="text-main-500">{DOCUMENT_FIELD}</span>
            <span className="min-w-0 truncate font-medium text-main-100" title={view.document}>
              {sourceName(view.document)}
            </span>
          </div>
          <Tags>
            {view.section === null ? null : <Field label={SECTION_FIELD}>{view.section}</Field>}
            {view.from === null ? null : (
              <Field label={RANGE_FIELD}>
                {view.to === null
                  ? `${String(view.from + 1)}…`
                  : `${String(view.from + 1)}–${String(view.to + 1)}`}
              </Field>
            )}
          </Tags>
        </div>
      );
    case "folder":
      return (
        <div className="space-y-2">
          <PathLine icon={mdiFolderOutline} path={view.path ?? WORKING_FOLDER} />
          <Tags>
            {view.recursive ? <Tag>{RECURSIVE_FLAG}</Tag> : null}
            {view.glob === null ? null : <Field label={FILTER_FIELD}>{view.glob}</Field>}
            {view.limit === null ? null : (
              <Field label={LIMIT_FIELD}>{view.limit.toLocaleString("ru-RU")}</Field>
            )}
          </Tags>
        </div>
      );
    case "files":
      return (
        <div>
          <Eyebrow>
            {FILES_TITLE} · {view.files.length.toLocaleString("ru-RU")}
          </Eyebrow>
          <ul className="m-0 list-none space-y-1 p-0">
            {view.files.map((file) => (
              <li
                key={`${file.path}:${String(file.start)}:${String(file.end)}`}
                className="flex min-w-0 items-center gap-2 rounded-control bg-main-900 px-2.5 py-1.5"
              >
                <Icon path={mdiFileOutline} size={14} className="shrink-0 text-info" />
                <span
                  className="min-w-0 flex-1 truncate font-mono text-[11.5px] text-main-200"
                  title={file.path}
                >
                  {file.path}
                </span>
                <Tag>{lineRange(file.start, file.end)}</Tag>
              </li>
            ))}
          </ul>
        </div>
      );
    case "grep":
      return (
        <div className="space-y-2">
          <QueryLine query={view.query} mono />
          <Tags>
            <Field label={FOLDER_FIELD}>{view.path ?? WORKING_FOLDER}</Field>
            {view.glob === null ? null : <Field label={FILTER_FIELD}>{view.glob}</Field>}
            {view.regex ? <Tag>{REGEX_FLAG}</Tag> : null}
            {view.ignoreCase ? <Tag>{IGNORE_CASE_FLAG}</Tag> : null}
            {view.limit === null ? null : (
              <Field label={LIMIT_FIELD}>{view.limit.toLocaleString("ru-RU")}</Field>
            )}
          </Tags>
        </div>
      );
    case "skill":
      return (
        <div className="space-y-2">
          <div className="flex min-w-0 flex-wrap items-center gap-2 text-[12px]">
            <span className="inline-flex items-center gap-1.5 rounded-pill bg-ok-soft px-2 py-0.5 font-medium text-ok">
              <Icon path={mdiPuzzleOutline} size={13} />
              <span className="sr-only">{SKILL_FIELD}</span>
              {view.skill}
            </span>
            {view.path === null ? null : (
              <span className="min-w-0 truncate font-mono text-[11.5px] text-main-300">
                {view.path}
              </span>
            )}
          </div>
          {view.args.length === 0 ? null : (
            <div className="rounded-control bg-main-900 px-2.5 py-2 font-mono text-[11.5px] break-all text-main-200">
              {commandLine(view.args)}
            </div>
          )}
          {view.stdin === null ? null : (
            <div>
              <Eyebrow>{STDIN_FIELD}</Eyebrow>
              <CodeBlock source={view.stdin} />
            </div>
          )}
        </div>
      );
    case "command":
      return (
        <div className="space-y-2">
          <div className="rounded-control bg-main-900 px-2.5 py-2 font-mono text-[12px] break-all text-main-100 fx-on:shadow-[inset_0_0_0_1px_var(--color-line)]">
            <span aria-hidden="true" className="text-accent-medium select-none">
              ${" "}
            </span>
            {commandLine(view.argv)}
          </div>
          {view.cwd === null ? null : (
            <Tags>
              <Field label={FOLDER_FIELD}>{view.cwd}</Field>
            </Tags>
          )}
        </div>
      );
    case "program":
      return (
        <div className="space-y-2">
          <Tags>
            {view.language === "" ? null : (
              <span className="rounded-pill bg-main-100 px-1.5 py-0.5 text-[11px] font-medium text-main-900">
                {view.language}
              </span>
            )}
            {view.packages.map((name) => (
              <Tag key={name} mono>
                {name}
              </Tag>
            ))}
            {view.args.length === 0 ? null : <Tag mono>{commandLine(view.args)}</Tag>}
          </Tags>
          <CodeBlock source={view.source} />
        </div>
      );
    case "fields":
      return (
        <dl className="m-0 grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-[11.5px]">
          {view.fields.map((field) => (
            <div key={field.label} className="contents">
              <dt className="font-mono text-main-500">{field.label}</dt>
              <dd className="m-0 min-w-0 font-mono break-all text-main-200">{field.value}</dd>
            </div>
          ))}
        </dl>
      );
  }
}

export function ToolCallViewBody({ view }: { readonly view: ToolCallView }) {
  return <ViewBody view={view} />;
}

export function RawArguments({ text }: { readonly text: string }) {
  const [open, setOpen] = useState(false);
  const id = useId();
  return (
    <div>
      <button
        type="button"
        aria-expanded={open}
        aria-controls={id}
        onClick={() => setOpen(!open)}
        className="inline-flex items-center gap-1 rounded-pill text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase hover:text-main-200"
      >
        {RAW_ARGUMENTS}
        <Icon
          path={mdiChevronDown}
          size={13}
          className={`anim-on:transition-transform ${open ? "rotate-180" : ""}`}
        />
      </button>
      <Collapse open={open} id={id}>
        <ScrollArea className="mt-1 max-h-48 rounded-control bg-main-900 px-2.5 py-2">
          <pre className="m-0 font-mono text-[11.5px] wrap-break-word whitespace-pre-wrap text-main-300">
            {text}
          </pre>
        </ScrollArea>
      </Collapse>
    </div>
  );
}

export function ResultBlock({
  result,
  truncated,
}: {
  readonly result: string;
  readonly truncated: boolean;
}) {
  const [copied, setCopied] = useState(false);
  useEffect(() => {
    if (!copied) return;
    const timer = window.setTimeout(() => setCopied(false), 2000);
    return () => window.clearTimeout(timer);
  }, [copied]);
  const lines = resultLines(result);
  return (
    <div>
      <div className="mb-1 flex items-center gap-2">
        <p className="m-0 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
          {RESULT_TITLE}
          {truncated ? ` · ${RESULT_SHORTENED}` : ""}
        </p>
        {lines > 1 ? (
          <span className="font-mono text-[10.5px] text-main-500 tabular-nums">
            {plural(lines, "строка", "строки", "строк")}
          </span>
        ) : null}
        <span className="ml-auto">
          <IconButton
            label={copied ? RESULT_COPIED : COPY_RESULT}
            tooltip="top-left"
            path={copied ? mdiCheck : mdiContentCopy}
            iconSize={14}
            onClick={() => {
              void navigator.clipboard
                ?.writeText(result)
                .then(() => setCopied(true))
                .catch(() => setCopied(false));
            }}
          />
        </span>
      </div>
      <ScrollArea className="max-h-60 rounded-control bg-main-900 px-2.5 py-2">
        <pre className="m-0 font-mono text-[11.5px] wrap-break-word whitespace-pre-wrap text-main-300">
          {result}
        </pre>
      </ScrollArea>
    </div>
  );
}
