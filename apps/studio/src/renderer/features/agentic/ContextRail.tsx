import { observer } from "mobx-react-lite";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { AgenticPermissionMode } from "@zvs/shared";
import Chip from "../../ui/atoms/Chip";
import SegmentedControl from "../../ui/atoms/SegmentedControl";
import {
  BUDGET_LABELS,
  BUDGET_TITLE,
  budgetValue,
  callsWord,
  CHANGED_EMPTY,
  CHANGED_TITLE,
  MODE_HINTS,
  MODE_LABELS,
  MODE_PENDING,
  MODE_TITLE,
  SESSION_TITLE,
  toolsWord,
  WORKSPACE_TITLE,
} from "./agenticCopy";
import type AgenticStore from "./AgenticStore";
import { relativePath } from "./sessionModel";

const MODE_OPTIONS = AgenticPermissionMode.options.map((value) => ({
  value,
  label: MODE_LABELS[value],
}));

function Eyebrow({ children }: { readonly children: string }) {
  return (
    <p className="m-0 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
      {children}
    </p>
  );
}

function initials(name: string): string {
  const letters = name
    .split(/\s+/)
    .filter((part) => part.length > 0)
    .map((part) => part[0] ?? "")
    .join("");
  return letters.slice(0, 2).toUpperCase();
}

export default observer(function ContextRail({ store }: { readonly store: AgenticStore }) {
  const session = store.session;
  if (session === null) return null;
  const mode = session.pendingMode ?? session.mode;
  const providerName = store.provider?.name ?? "";
  return (
    <aside
      aria-label={SESSION_TITLE}
      className="flex w-78 flex-none flex-col border-l border-line bg-main-800"
    >
      <ScrollArea className="flex min-h-0 flex-1 flex-col gap-3 px-3.5 py-4">
        <Eyebrow>{SESSION_TITLE}</Eyebrow>
        <div className="flex flex-col gap-2.5 rounded-card bg-main-900 p-3 shadow-card">
          <div className="flex items-center gap-2.5">
            <span className="flex size-7 flex-none items-center justify-center rounded-control bg-accent-soft text-[10.5px] font-semibold text-accent-medium">
              {initials(providerName.length > 0 ? providerName : store.modelName)}
            </span>
            <div className="min-w-0 flex-1">
              <p
                className="m-0 truncate font-mono text-[12px] text-main-50"
                title={session.modelId}
              >
                {store.modelName}
              </p>
              {providerName.length > 0 ? (
                <p className="m-0 truncate text-[11px] text-main-500">{providerName}</p>
              ) : null}
            </div>
          </div>
          <div className="flex flex-wrap gap-1.5">
            <Chip>{toolsWord(session.tools.length)}</Chip>
            <Chip>{callsWord(session.usage.toolCalls)}</Chip>
          </div>
        </div>

        <div className="flex flex-col gap-1.5">
          <Eyebrow>{MODE_TITLE}</Eyebrow>
          {store.editable ? (
            <SegmentedControl
              label={MODE_TITLE}
              value={mode}
              options={MODE_OPTIONS}
              onChange={(next) => void store.setMode(next)}
            />
          ) : (
            <p className="m-0 text-[12.5px] text-main-200">{MODE_LABELS[mode]}</p>
          )}
          <p
            className={`m-0 text-[11.5px] ${session.pendingMode === null ? "text-main-500" : "text-warn"}`}
          >
            {session.pendingMode === null ? MODE_HINTS[mode] : MODE_PENDING}
          </p>
        </div>

        <div className="flex flex-col gap-1.5">
          <Eyebrow>{WORKSPACE_TITLE}</Eyebrow>
          <p className="m-0 rounded-card bg-main-900 px-3 py-2.5 font-mono text-[11px] break-all text-main-300 shadow-card">
            {session.workspace}
          </p>
        </div>

        <div className="flex flex-col gap-1.5">
          <Eyebrow>
            {session.changedFiles.length === 0
              ? CHANGED_TITLE
              : `${CHANGED_TITLE} · ${session.changedFiles.length.toLocaleString("ru-RU")}`}
          </Eyebrow>
          {session.changedFiles.length === 0 ? (
            <p className="m-0 text-[12px] text-main-500">{CHANGED_EMPTY}</p>
          ) : (
            <ul className="m-0 flex list-none flex-col gap-1.5 p-0">
              {session.changedFiles.map((file) => (
                <li
                  key={file.path}
                  className="flex items-center gap-2 rounded-card bg-main-900 px-2.5 py-2 shadow-card"
                >
                  <span
                    className="min-w-0 flex-1 truncate font-mono text-[11px] text-main-200"
                    title={file.path}
                  >
                    {relativePath(file.path, session.workspace)}
                  </span>
                  <span className="flex-none font-mono text-[10.5px] text-ok tabular-nums">
                    +{file.added.toLocaleString("ru-RU")}
                  </span>
                  <span className="flex-none font-mono text-[10.5px] text-err tabular-nums">
                    −{file.removed.toLocaleString("ru-RU")}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </div>

        <div className="mt-auto flex flex-col gap-2.5 rounded-card bg-main-900 p-3 shadow-card">
          <Eyebrow>{BUDGET_TITLE}</Eyebrow>
          {store.budget.map((line) => (
            <div key={line.key} className="flex flex-col gap-1">
              <div className="flex justify-between gap-2 text-[11px]">
                <span className="text-main-400">{BUDGET_LABELS[line.key]}</span>
                <span className="font-mono text-main-300 tabular-nums">
                  {budgetValue(line.key, line.used, line.limit)}
                </span>
              </div>
              <div className="h-1 rounded-full bg-main-700">
                <div
                  className={`h-full rounded-full ${line.ratio >= 1 ? "bg-warn" : "bg-accent-dark"}`}
                  style={{ width: `${String(line.ratio * 100)}%` }}
                />
              </div>
            </div>
          ))}
        </div>
      </ScrollArea>
    </aside>
  );
});
