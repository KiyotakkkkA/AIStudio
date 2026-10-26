import { useState } from "react";
import { mdiCheck, mdiChevronDown, mdiContentCopy, mdiWeb } from "@mdi/js";
import { Dropdown, ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import type { WebSourceDto } from "@zvs/shared";
import IconButton from "../../ui/atoms/buttons/IconButton";
import Icon from "../../ui/atoms/Icon";
import {
  COPY_LINK,
  LINK_COPIED,
  WEB_SOURCES_TITLE,
  webSourceDetail,
  webSourceHosts,
  webSourcesLabel,
} from "./webSources";

function SourceRow({ source, number }: { readonly source: WebSourceDto; readonly number: number }) {
  const [copied, setCopied] = useState(false);
  return (
    <li className="flex items-start gap-2.5 rounded-control px-2 py-1.75 hover:bg-main-850">
      <span className="mt-0.5 flex h-4.5 min-w-4.5 flex-none items-center justify-center rounded-full bg-accent-soft px-1 font-mono text-[10.5px] font-semibold text-accent-medium tabular-nums">
        {number}
      </span>
      <span className="min-w-0 flex-1">
        <span title={source.url} className="block truncate text-[12.5px] font-medium text-main-100">
          {source.title}
        </span>
        <span className="block truncate text-[11.5px] text-main-500">
          {webSourceDetail(source)}
        </span>
        {source.snippet && (
          <span className="mt-0.5 line-clamp-2 text-[11.5px] text-main-400">{source.snippet}</span>
        )}
      </span>
      <IconButton
        label={copied ? LINK_COPIED : COPY_LINK}
        tooltip="top-left"
        path={copied ? mdiCheck : mdiContentCopy}
        iconSize={14}
        onClick={() => {
          void navigator.clipboard
            ?.writeText(source.url)
            .then(() => setCopied(true))
            .catch(() => setCopied(false));
        }}
      />
    </li>
  );
}

export default function ChatWebSources({ sources }: { readonly sources: readonly WebSourceDto[] }) {
  if (sources.length === 0) return null;
  const hosts = webSourceHosts(sources);
  return (
    <Dropdown menuWidth={420} menuPlacement="bottom-left">
      <Dropdown.Trigger
        title={WEB_SOURCES_TITLE}
        rounded="rounded-full"
        icon={<Icon path={mdiChevronDown} size={14} className="shrink-0 text-main-500" />}
        className="h-7 w-auto max-w-full min-w-0 gap-1.5 border border-line bg-main-850 px-2.5 text-[12px] text-main-300 hover:bg-main-800 hover:text-main-100"
      >
        <span className="flex min-w-0 items-center gap-1.5">
          <Icon path={mdiWeb} size={14} className="shrink-0 text-accent-medium" />
          <span className="truncate">{hosts.join(", ")}</span>
          <span className="shrink-0 text-main-500">· {webSourcesLabel(sources.length)}</span>
        </span>
      </Dropdown.Trigger>
      <Dropdown.Menu
        aria-label={WEB_SOURCES_TITLE}
        rounded=""
        className="overflow-hidden rounded-card border-line bg-main-900 p-0 shadow-pop"
      >
        <p className="px-3.5 pt-3 pb-1.5 text-[13px] font-semibold text-main-50">
          {WEB_SOURCES_TITLE}
        </p>
        <ScrollArea className="max-h-80 px-1.5 pb-1.5">
          <ol className="m-0 list-none space-y-0.5 p-0">
            {sources.map((source, index) => (
              <SourceRow
                key={`${String(index)}:${source.url}`}
                source={source}
                number={index + 1}
              />
            ))}
          </ol>
        </ScrollArea>
      </Dropdown.Menu>
    </Dropdown>
  );
}
