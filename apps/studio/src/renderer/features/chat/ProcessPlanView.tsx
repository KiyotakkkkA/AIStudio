import type { ProcessPlanDto } from "@zvs/shared";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import {
  ARGS_TITLE,
  COMMAND_TITLE,
  FOLDER_TITLE,
  LANGUAGE_LABEL,
  LANGUAGE_TITLE,
  PACKAGES_TITLE,
  PROGRAM_TITLE,
  SOURCE_TITLE,
  commandLine,
} from "./processCopy";

function Row({ title, value }: { readonly title: string; readonly value: string }) {
  return (
    <div className="flex min-w-0 gap-2 text-[11.5px]">
      <dt className="w-24 shrink-0 text-main-500">{title}</dt>
      <dd className="m-0 min-w-0 flex-1 font-mono break-all text-main-200">{value}</dd>
    </div>
  );
}

export default function ProcessPlanView({
  plan,
  expanded = false,
}: {
  readonly plan: ProcessPlanDto;
  readonly expanded?: boolean;
}) {
  const code = plan.language !== undefined;
  const body = code ? (plan.source ?? "") : commandLine(plan.argv);
  return (
    <div className="space-y-2">
      <dl className="m-0 space-y-1">
        {plan.language !== undefined && (
          <Row title={LANGUAGE_TITLE} value={LANGUAGE_LABEL[plan.language]} />
        )}
        <Row title={PROGRAM_TITLE} value={plan.program} />
        <Row title={FOLDER_TITLE} value={plan.cwd} />
        {plan.packages.length > 0 && (
          <Row title={PACKAGES_TITLE} value={plan.packages.join(", ")} />
        )}
        {code && plan.argv.length > 0 && <Row title={ARGS_TITLE} value={commandLine(plan.argv)} />}
      </dl>
      <div>
        <p className="mb-1 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
          {code ? SOURCE_TITLE : COMMAND_TITLE}
        </p>
        <ScrollArea
          className={`${expanded ? "max-h-96" : "max-h-48"} rounded-control bg-main-900 px-2.5 py-2`}
        >
          <pre className="m-0 font-mono text-[11.5px] wrap-break-word whitespace-pre-wrap text-main-200">
            {body}
          </pre>
        </ScrollArea>
      </div>
    </div>
  );
}
