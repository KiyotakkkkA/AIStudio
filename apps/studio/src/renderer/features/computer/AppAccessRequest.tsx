import type { ComputerAppDto, FolderAccessAnswer } from "@zvs/shared";
import Button from "../../ui/atoms/buttons/Button";
import AppIcon from "./AppIcon";
import { ACCESS_ANSWERS, ACCESS_REQUEST_LEAD, PATH_UNKNOWN } from "./computerCopy";

export interface AppAccessRequestProps {
  readonly apps: readonly ComputerAppDto[];
  readonly detail?: string | undefined;
  readonly busy: boolean;
  readonly onAnswer: (answer: FolderAccessAnswer) => void;
}

export default function AppAccessRequest({ apps, detail, busy, onAnswer }: AppAccessRequestProps) {
  return (
    <div role="status" className="flex flex-col gap-3 rounded-control bg-warn-soft px-3 py-2.5">
      <p className="m-0 text-[12.5px] text-main-100">{ACCESS_REQUEST_LEAD}</p>
      {detail === undefined || detail.length === 0 ? null : (
        <p className="m-0 text-[12px] text-main-300">{detail}</p>
      )}
      <ul className="m-0 flex list-none flex-col gap-1.5 p-0">
        {apps.map((app) => (
          <li
            key={app.app}
            className="flex min-w-0 items-center gap-2.5 rounded-control bg-main-900 px-2.5 py-2"
          >
            <AppIcon icon={app.icon} />
            <span className="flex min-w-0 flex-col">
              <span className="text-[12.5px] font-semibold text-main-50">{app.app}</span>
              <span
                className="truncate font-mono text-[11px] text-main-400"
                title={app.path.length > 0 ? app.path : undefined}
              >
                {app.path.length > 0 ? app.path : PATH_UNKNOWN}
              </span>
            </span>
          </li>
        ))}
      </ul>
      <div className="flex flex-wrap gap-2">
        {ACCESS_ANSWERS.map(({ answer, label, tone }) => (
          <Button
            key={answer}
            size="sm"
            tone={tone}
            disabled={busy}
            onClick={() => onAnswer(answer)}
          >
            {label}
          </Button>
        ))}
      </div>
    </div>
  );
}
