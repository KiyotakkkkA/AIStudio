import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import {
  ACCOUNT_TITLE,
  CC_TITLE,
  FROM_TITLE,
  NO_SUBJECT,
  REPLY_TITLE,
  SUBJECT_TITLE,
  TEXT_TITLE,
  TO_TITLE,
  type MailPreview,
} from "./mailCopy";

function Row({ title, value }: { readonly title: string; readonly value: string }) {
  return (
    <div className="flex min-w-0 gap-2 text-[11.5px]">
      <dt className="w-20 shrink-0 text-main-500">{title}</dt>
      <dd className="m-0 min-w-0 flex-1 break-all text-main-200">{value}</dd>
    </div>
  );
}

export default function MailDraftView({
  preview,
  expanded = false,
}: {
  readonly preview: MailPreview;
  readonly expanded?: boolean;
}) {
  const { draft } = preview;
  return (
    <div className="space-y-2">
      <dl className="m-0 space-y-1">
        <Row title={ACCOUNT_TITLE} value={preview.account} />
        <Row title={FROM_TITLE} value={draft.from} />
        <Row title={TO_TITLE} value={draft.to.join(", ")} />
        {draft.cc.length > 0 && <Row title={CC_TITLE} value={draft.cc.join(", ")} />}
        <Row title={SUBJECT_TITLE} value={draft.subject.length > 0 ? draft.subject : NO_SUBJECT} />
        {draft.inReplyTo !== null && <Row title={REPLY_TITLE} value={draft.inReplyTo} />}
      </dl>
      <div>
        <p className="mb-1 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
          {TEXT_TITLE}
        </p>
        <ScrollArea
          className={`${expanded ? "max-h-96" : "max-h-48"} rounded-control bg-main-800 px-2.5 py-2`}
        >
          <pre className="m-0 font-sans text-[12px] wrap-break-word whitespace-pre-wrap text-main-200">
            {draft.text}
          </pre>
        </ScrollArea>
      </div>
    </div>
  );
}
