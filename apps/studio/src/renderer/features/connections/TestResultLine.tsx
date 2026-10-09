import { mdiAlertCircleOutline, mdiCheck, mdiInformationOutline } from "@mdi/js";
import type { McpTestResultDto } from "@zvs/shared";
import Icon from "../../ui/atoms/Icon";
import { failureCopy, handshakeLine } from "./connectionPresentation";

export interface TestResultLineProps {
  readonly result: McpTestResultDto | null;
  readonly testing: boolean;
  readonly unsaved: boolean;
  readonly lastError: string | null;
}

export default function TestResultLine({
  result,
  testing,
  unsaved,
  lastError,
}: TestResultLineProps) {
  if (testing) {
    return (
      <span className="flex items-center gap-1.75 text-[12px] text-main-400">
        <Icon path={mdiInformationOutline} size={14} className="flex-none" />
        Рукопожатие…
      </span>
    );
  }

  if (result === null && lastError !== null) {
    const copy = failureCopy(lastError);
    return (
      <div className="flex min-w-0 flex-col gap-0.5">
        <span className="truncate text-[12px] text-err" title={copy.title}>
          {copy.title}
        </span>
        {copy.hint === null ? null : <span className="text-[11px] text-main-500">{copy.hint}</span>}
      </div>
    );
  }

  if (result === null) {
    return (
      <span className="text-[12px] text-main-500">
        {unsaved
          ? "Перед проверкой изменения будут сохранены."
          : "Проверка заново подключит сервер и обновит список инструментов."}
      </span>
    );
  }

  if (result.ok) {
    return (
      <span className="flex min-w-0 items-center gap-1.75 text-[12px]">
        <Icon path={mdiCheck} size={14} className="flex-none text-ok" />
        <span className="truncate font-mono text-main-300">{handshakeLine(result)}</span>
      </span>
    );
  }

  const copy = failureCopy(result.error);
  return (
    <div className="flex min-w-0 flex-col gap-0.5">
      <span className="flex min-w-0 items-center gap-1.75 text-[12px]">
        <Icon path={mdiAlertCircleOutline} size={14} className="flex-none text-err" />
        <span className="font-mono text-err">handshake failed</span>
        <span className="truncate text-main-300" title={copy.title}>
          {copy.title}
        </span>
      </span>
      {copy.hint === null ? null : <span className="text-[11px] text-main-500">{copy.hint}</span>}
    </div>
  );
}
