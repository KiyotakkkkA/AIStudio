import type { ToolchainDto } from "@zvs/shared";
import IconButton from "../../ui/atoms/buttons/IconButton";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import StatusDot from "../../ui/atoms/StatusDot";
import { STATUS_TONE_TEXT } from "../../ui/atoms/statusTone";
import {
  ACTION_ICONS,
  ACTION_LABELS,
  INSTALLING_HINT,
  RUNTIME_HINTS,
  RUNTIME_ICONS,
  SOURCE_LABELS,
  STATUS_LABELS,
  STATUS_TONES,
  availableActions,
  removalCopy,
  removalLabel,
  updateLine,
  versionLine,
  type ToolchainAction,
} from "./toolchainPresentation";

const PRIMARY_TONE =
  "zvs-press border border-transparent bg-accent-dark text-on-accent hover:bg-accent-medium";
const SECONDARY_TONE =
  "zvs-press border border-line text-main-300 hover:border-line-strong hover:bg-main-850 hover:text-main-100";

export interface ToolchainCardProps {
  readonly dto: ToolchainDto;
  readonly busy: ToolchainAction | undefined;
  readonly error: string | undefined;
  readonly onAction: (action: ToolchainAction) => void;
}

export default function ToolchainCard({ dto, busy, error, onAction }: ToolchainCardProps) {
  const tone = STATUS_TONES[dto.status];
  const actions = availableActions(dto);
  const update = updateLine(dto);

  return (
    <section
      aria-label={dto.displayName}
      className="flex flex-col gap-3 rounded-card border border-line bg-main-900 px-4 py-3.5"
    >
      <header className="flex items-center gap-3">
        <span className="flex size-9 flex-none items-center justify-center rounded-control bg-main-700 text-main-300">
          <Icon path={RUNTIME_ICONS[dto.runtime]} size={18} />
        </span>
        <span className="flex min-w-0 flex-1 flex-col gap-0.5">
          <span className="truncate text-[14px] font-semibold text-main-50">{dto.displayName}</span>
          <span className="truncate text-[12px] text-main-400">{RUNTIME_HINTS[dto.runtime]}</span>
        </span>
        <Chip title={STATUS_LABELS[dto.status]}>
          <StatusDot tone={tone} />
          <span className={STATUS_TONE_TEXT[tone]}>{STATUS_LABELS[dto.status]}</span>
        </Chip>
      </header>

      <dl className="m-0 grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5 text-[12px]">
        <dt className="text-main-500">Версия</dt>
        <dd className="m-0 font-mono text-main-200">{versionLine(dto)}</dd>
        <dt className="text-main-500">Источник</dt>
        <dd className="m-0 text-main-200">
          {dto.source === undefined ? "—" : SOURCE_LABELS[dto.source]}
        </dd>
        <dt className="text-main-500">Файл</dt>
        <dd
          className="m-0 truncate font-mono text-main-200"
          title={dto.executablePath ?? undefined}
        >
          {dto.executablePath ?? "—"}
        </dd>
      </dl>

      {update === undefined ? null : <p className="m-0 text-[12px] text-warn">{update}</p>}
      {dto.status === "installing" ? (
        <p className="m-0 text-[12px] text-main-400">{INSTALLING_HINT}</p>
      ) : null}
      {dto.problem === undefined ? null : <p className="m-0 text-[12px] text-err">{dto.problem}</p>}
      {error === undefined ? null : (
        <p role="alert" className="m-0 text-[12px] text-err">
          {error}
        </p>
      )}

      {actions.length === 0 ? null : (
        <div className="flex flex-wrap gap-2">
          {actions.map((action) =>
            action === "remove" ? (
              <IconButton
                key={action}
                tone="danger"
                size="md"
                path={ACTION_ICONS[action]}
                label={removalLabel(dto)}
                disabled={busy !== undefined}
                needConfirm
                modalSetup={{
                  title: `${removalLabel(dto)} ${dto.displayName}?`,
                  content: removalCopy(dto.displayName),
                  confirmLabel: removalLabel(dto),
                }}
                onClick={() => onAction(action)}
              />
            ) : (
              <IconButton
                key={action}
                size="md"
                path={ACTION_ICONS[action]}
                label={busy === action ? `${ACTION_LABELS[action]}…` : ACTION_LABELS[action]}
                toneClassName={
                  action === "install" || action === "update" ? PRIMARY_TONE : SECONDARY_TONE
                }
                disabled={busy !== undefined}
                onClick={() => onAction(action)}
              />
            ),
          )}
        </div>
      )}
    </section>
  );
}
