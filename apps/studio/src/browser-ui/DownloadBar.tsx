import type { BrowserCommand, BrowserPendingDownload } from "@zvs/shared";
import { mdiTrayArrowDown } from "@mdi/js";
import Icon from "./Icon";
import { DOWNLOAD_DISCARD, DOWNLOAD_KEEP, downloadDetail, downloadTitle } from "./downloadCopy";

interface DownloadBarProps {
  readonly downloads: readonly BrowserPendingDownload[];
  readonly send: (command: BrowserCommand) => void;
}

export default function DownloadBar({ downloads, send }: DownloadBarProps) {
  const current = downloads.find((download) => download.ready) ?? downloads[0];
  if (current === undefined) return null;
  return (
    <div className="approval-bar download-bar" role="alertdialog" aria-labelledby="download-title">
      <span className="approval-mark download-mark" aria-hidden="true">
        <Icon path={mdiTrayArrowDown} size={19} />
      </span>
      <div className="approval-text">
        <strong id="download-title">{downloadTitle(current)}</strong>
        <code>{downloadDetail(current, downloads.length - 1)}</code>
      </div>
      <button
        type="button"
        className="ghost"
        disabled={!current.ready}
        onClick={() => send({ action: "downloadDiscard", id: current.id })}
      >
        {DOWNLOAD_DISCARD}
      </button>
      <button
        type="button"
        className="primary"
        disabled={!current.ready}
        onClick={() => send({ action: "downloadKeep", id: current.id })}
      >
        {DOWNLOAD_KEEP}
      </button>
    </div>
  );
}
