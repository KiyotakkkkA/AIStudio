import { useState } from "react";
import { mdiCheck, mdiContentCopy, mdiOpenInNew } from "@mdi/js";
import type { WebSourceDto } from "@zvs/shared";
import IconButton from "../../ui/atoms/buttons/IconButton";
import SiteIcon from "./SiteIcon";
import { COPY_LINK, LINK_COPIED, OPEN_SITE, webSourceDetail } from "./webSources";

export default function WebSourceCard({
  source,
  number,
  onOpen,
}: {
  readonly source: WebSourceDto;
  readonly number: number;
  readonly onOpen: (source: WebSourceDto) => void;
}) {
  const [copied, setCopied] = useState(false);
  return (
    <article className="rounded-card border border-line bg-main-900 p-3.5">
      <div className="flex items-start justify-between gap-3">
        <h3 className="flex min-w-0 items-center gap-2 text-[13px] font-semibold text-main-100">
          <span className="shrink-0 rounded-full bg-main-800 px-1.75 py-0.5 font-mono text-[11px] text-main-400 tabular-nums">
            [{number}]
          </span>
          <SiteIcon source={source} size="md" />
          <span title={source.url} className="truncate">
            {source.title}
          </span>
        </h3>
        <span className="flex shrink-0 items-center gap-1">
          <IconButton
            label={OPEN_SITE}
            tooltip="top-right"
            path={mdiOpenInNew}
            iconSize={16}
            onClick={() => onOpen(source)}
          />
          <IconButton
            label={copied ? LINK_COPIED : COPY_LINK}
            tooltip="top-right"
            path={copied ? mdiCheck : mdiContentCopy}
            iconSize={16}
            onClick={() => {
              void navigator.clipboard
                ?.writeText(source.url)
                .then(() => setCopied(true))
                .catch(() => setCopied(false));
            }}
          />
        </span>
      </div>
      <p className="mt-1.5 text-[12px] text-main-500">{webSourceDetail(source)}</p>
      {source.snippet && (
        <p className="mt-2 line-clamp-4 border-l-2 border-accent-line pl-2.5 text-[12.5px]/relaxed text-main-300">
          {source.snippet}
        </p>
      )}
    </article>
  );
}
