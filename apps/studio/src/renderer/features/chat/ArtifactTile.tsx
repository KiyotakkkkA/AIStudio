import type { ReactNode } from "react";
import {
  mdiFileDocumentOutline,
  mdiFileExcelOutline,
  mdiFileImageOutline,
  mdiFilePdfBox,
  mdiFilePowerpointOutline,
  mdiFileWordOutline,
} from "@mdi/js";
import Icon from "../../ui/atoms/Icon";
import { STATUS_TONE_TEXT, type StatusTone } from "../../ui/atoms/statusTone";
import { artifactKind } from "./artifacts";

const TILE_BACKGROUNDS: Record<StatusTone, string> = {
  ok: "bg-ok-soft",
  warn: "bg-warn-soft",
  err: "bg-err-soft",
  idle: "bg-main-700",
  accent: "bg-accent-soft",
};

function formatIcon(format: string): string {
  switch (format.toLowerCase()) {
    case "pdf":
      return mdiFilePdfBox;
    case "docx":
    case "doc":
    case "odt":
    case "rtf":
      return mdiFileWordOutline;
    case "xlsx":
    case "xlsm":
    case "xls":
    case "ods":
    case "csv":
      return mdiFileExcelOutline;
    case "pptx":
    case "ppt":
    case "odp":
      return mdiFilePowerpointOutline;
    case "png":
    case "jpg":
    case "jpeg":
    case "svg":
      return mdiFileImageOutline;
    default:
      return mdiFileDocumentOutline;
  }
}

export default function ArtifactTile({
  name,
  format,
  subtitle,
  actions,
  error,
  notice,
}: {
  readonly name: string;
  readonly format: string;
  readonly subtitle: string;
  readonly actions: ReactNode;
  readonly error?: string | undefined;
  readonly notice?: string | undefined;
}) {
  const tone = artifactKind(format).tone;
  return (
    <li className="flex max-w-120 flex-col gap-1.5 rounded-card border border-line bg-main-900 px-3 py-2.5">
      <div className="flex items-center gap-3">
        <span
          aria-hidden="true"
          className={`flex size-10 flex-none items-center justify-center rounded-control ${TILE_BACKGROUNDS[tone]} ${STATUS_TONE_TEXT[tone]}`}
        >
          <Icon path={formatIcon(format)} size={20} />
        </span>
        <span className="flex min-w-0 flex-1 flex-col gap-0.5">
          <span className="truncate text-[13.5px] font-semibold text-main-50" title={name}>
            {name}
          </span>
          <span className="truncate text-[12px] text-main-500">{subtitle}</span>
        </span>
        <span className="flex flex-none items-center gap-1">{actions}</span>
      </div>
      {error === undefined ? null : (
        <p role="alert" className="m-0 text-[12px] text-err">
          {error}
        </p>
      )}
      {notice === undefined ? null : (
        <p role="status" className="m-0 text-[12px] text-ok">
          {notice}
        </p>
      )}
    </li>
  );
}
