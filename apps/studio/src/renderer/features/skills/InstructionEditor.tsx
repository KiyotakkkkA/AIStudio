import { useMemo, useState } from "react";
import Chip from "../../ui/atoms/Chip";
import { markdownLines, type TokenKind } from "./markdownTokens";
import { linesWord } from "./skillPresentation";

const TOKEN_CLASS: Record<TokenKind, string> = {
  plain: "",
  heading: "font-semibold text-main-50",
  marker: "text-accent-medium",
  code: "text-accent-medium",
  strong: "font-semibold text-main-100",
  emphasis: "italic",
  fence: "text-main-500",
  quote: "text-main-400",
};

const SURFACE = "m-0 px-3.25 py-2.75 font-mono text-[12px] leading-[1.75] whitespace-pre";

export interface InstructionEditorProps {
  readonly value: string;
  readonly invalid?: boolean;
  readonly onChange: (value: string) => void;
}

export default function InstructionEditor({
  value,
  invalid = false,
  onChange,
}: InstructionEditorProps) {
  const [scroll, setScroll] = useState({ top: 0, left: 0 });
  const lines = useMemo(() => markdownLines(value), [value]);

  return (
    <div
      className={`flex min-h-0 flex-1 flex-col overflow-hidden rounded-card border bg-main-900 ${
        invalid ? "border-err" : "border-line focus-within:border-accent-dark"
      }`}
    >
      <div className="flex h-7.5 flex-none items-center gap-2 border-b border-line px-2.75">
        <span className="font-mono text-[10.5px] text-main-500">SKILL.md</span>
        <span className="flex-1" />
        <Chip>markdown</Chip>
        <Chip>{linesWord(lines.length)}</Chip>
      </div>
      <div className="relative flex min-h-0 flex-1">
        <div
          aria-hidden
          className="relative w-10 flex-none overflow-hidden border-r border-line bg-main-850 text-right select-none"
        >
          <div
            className={`${SURFACE} px-2 text-main-500`}
            style={{ transform: `translateY(${String(-scroll.top)}px)` }}
          >
            {lines.map((_, index) => (
              <div key={index}>{index + 1}</div>
            ))}
          </div>
        </div>
        <div className="relative min-w-0 flex-1 overflow-hidden">
          <pre
            aria-hidden
            className={`${SURFACE} pointer-events-none absolute inset-0 text-main-300`}
            style={{ transform: `translate(${String(-scroll.left)}px, ${String(-scroll.top)}px)` }}
          >
            {lines.map((tokens, index) => (
              <div key={index}>
                {tokens.length === 0 ? "​" : null}
                {tokens.map((token, at) => (
                  <span key={at} className={TOKEN_CLASS[token.kind]}>
                    {token.text}
                  </span>
                ))}
              </div>
            ))}
          </pre>
          <textarea
            aria-label="Инструкции SKILL.md"
            spellCheck={false}
            value={value}
            wrap="off"
            className={`${SURFACE} absolute inset-0 size-full resize-none overflow-auto border-0 bg-transparent text-transparent caret-main-100 outline-none selection:bg-accent-soft selection:text-transparent`}
            onChange={(event) => {
              onChange(event.target.value);
            }}
            onScroll={(event) => {
              setScroll({
                top: event.currentTarget.scrollTop,
                left: event.currentTarget.scrollLeft,
              });
            }}
            onKeyDown={(event) => {
              if (event.key !== "Tab" || event.shiftKey) return;
              event.preventDefault();
              const target = event.currentTarget;
              const start = target.selectionStart;
              const end = target.selectionEnd;
              const next = `${value.slice(0, start)}  ${value.slice(end)}`;
              onChange(next);
              requestAnimationFrame(() => {
                target.selectionStart = start + 2;
                target.selectionEnd = start + 2;
              });
            }}
          />
        </div>
      </div>
    </div>
  );
}
