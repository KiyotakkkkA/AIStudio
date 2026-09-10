import { mdiAlertCircleOutline, mdiCheck, mdiInformationOutline } from "@mdi/js";
import type { ProbeResultDto } from "@zvs/shared";
import Icon from "../../ui/atoms/Icon";
import { STATUS_TONE_TEXT } from "../../ui/atoms/statusTone";
import { checkedAgo, probeLine } from "./providerPresentation";

export interface ProbeResultLineProps {
  readonly result: ProbeResultDto | null;
  readonly probing: boolean;
  /** Shown instead of a bad-key message when the outcome is about the vendor session. */
  readonly relinkNote: string;
}

export default function ProbeResultLine({ result, probing, relinkNote }: ProbeResultLineProps) {
  if (probing) {
    return (
      <span className="flex items-center gap-1.75 text-[11.5px] text-main-400">
        <Icon path={mdiInformationOutline} size={14} className="flex-none" />
        Проверяем связь…
      </span>
    );
  }

  if (result === null) {
    return (
      <span className="text-[11.5px] text-main-500">
        Связь ещё не проверялась — модели появятся после проверки.
      </span>
    );
  }

  const line = probeLine(result.outcome);

  return (
    <div className="flex min-w-0 flex-col gap-0.5">
      <span className="flex min-w-0 items-center gap-1.75 text-[11.5px]">
        <Icon
          path={line.tone === "ok" ? mdiCheck : mdiAlertCircleOutline}
          size={14}
          className={`flex-none ${STATUS_TONE_TEXT[line.tone]}`}
        />
        <span className={`font-mono ${STATUS_TONE_TEXT[line.tone]}`}>{line.text}</span>
        <span className="truncate text-main-500">{checkedAgo(result, Date.now())}</span>
      </span>
      {line.detail === null ? null : (
        <span className="text-[11px] text-main-500">
          {line.detail}
          {line.relink ? <span className="ml-1 text-accent-dark">{relinkNote}</span> : null}
        </span>
      )}
    </div>
  );
}
