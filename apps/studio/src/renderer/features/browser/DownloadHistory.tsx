import { observer } from "mobx-react-lite";
import { useEffect } from "react";
import { mdiClose, mdiFileOutline, mdiFolderOutline, mdiOpenInNew, mdiRobotOutline } from "@mdi/js";
import Button from "../../ui/atoms/buttons/Button";
import IconButton from "../../ui/atoms/buttons/IconButton";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import { useStore } from "../../stores/useStore";
import BrowserSubPage from "./BrowserSubPage";
import {
  DOWNLOAD_BY_AGENT,
  DOWNLOAD_HISTORY_HINT,
  DOWNLOAD_HISTORY_LABEL,
  DOWNLOAD_OPEN,
  DOWNLOAD_REMOVE,
  DOWNLOAD_REVEAL,
  DOWNLOAD_STATE_LABELS,
  DOWNLOADS_CLEAR,
  DOWNLOADS_CLEAR_CONSEQUENCE,
  DOWNLOADS_CLEAR_TITLE,
  DOWNLOADS_EMPTY,
  downloadSize,
  downloadTime,
} from "./browserDataCopy";
import { siteHost } from "./browserSitesCopy";

export default observer(function DownloadHistory({ onBack }: { readonly onBack: () => void }) {
  const { browserDownloads } = useStore();
  useEffect(() => {
    void browserDownloads.load();
  }, [browserDownloads]);
  const items = browserDownloads.items;
  return (
    <BrowserSubPage
      title={DOWNLOAD_HISTORY_LABEL}
      hint={DOWNLOAD_HISTORY_HINT}
      error={browserDownloads.error}
      onDismissError={browserDownloads.dismissError}
      onBack={onBack}
      actions={
        <Button
          tone="secondary"
          size="sm"
          disabled={items.length === 0 || browserDownloads.busy}
          needConfirm
          modalSetup={{
            title: DOWNLOADS_CLEAR_TITLE,
            content: DOWNLOADS_CLEAR_CONSEQUENCE,
            confirmLabel: DOWNLOADS_CLEAR,
          }}
          onClick={() => void browserDownloads.clear()}
        >
          {DOWNLOADS_CLEAR}
        </Button>
      }
    >
      <div className="rounded-card border border-line bg-main-850">
        {items.length === 0 ? (
          <p className="m-0 px-4 py-6 text-center text-[12.5px] text-main-500">
            {browserDownloads.loaded ? DOWNLOADS_EMPTY : null}
          </p>
        ) : (
          <ul className="m-0 list-none p-0">
            {items.map((item) => (
              <li
                key={item.id}
                className="flex items-center gap-3 border-b border-line px-4 py-2.5 last:border-b-0"
              >
                <Icon path={mdiFileOutline} size={18} className="flex-none text-main-400" />
                <div className="flex min-w-0 flex-1 flex-col gap-0.5">
                  <span className="truncate text-[13px] text-main-100" title={item.fileName}>
                    {item.fileName}
                  </span>
                  <span className="truncate text-[11.5px] text-main-500">
                    {[
                      siteHost(item.url),
                      item.state === "done"
                        ? downloadSize(item.bytes)
                        : DOWNLOAD_STATE_LABELS[item.state],
                      downloadTime(item.finishedAt),
                    ].join(" · ")}
                  </span>
                </div>
                {item.agent ? (
                  <Chip>
                    <Icon path={mdiRobotOutline} size={12} />
                    {DOWNLOAD_BY_AGENT}
                  </Chip>
                ) : null}
                {item.path === null ? null : (
                  <>
                    <IconButton
                      path={mdiOpenInNew}
                      label={DOWNLOAD_OPEN}
                      size="sm"
                      onClick={() => void browserDownloads.open(item.id)}
                    />
                    <IconButton
                      path={mdiFolderOutline}
                      label={DOWNLOAD_REVEAL}
                      size="sm"
                      onClick={() => void browserDownloads.reveal(item.id)}
                    />
                  </>
                )}
                <IconButton
                  path={mdiClose}
                  label={DOWNLOAD_REMOVE}
                  size="sm"
                  disabled={browserDownloads.busy}
                  onClick={() => void browserDownloads.remove(item.id)}
                />
              </li>
            ))}
          </ul>
        )}
      </div>
    </BrowserSubPage>
  );
});
