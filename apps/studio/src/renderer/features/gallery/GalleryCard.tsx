import { useEffect, useRef, useState } from "react";
import { observer } from "mobx-react-lite";
import { Dropdown } from "@kiyotakkkka/zvs-uikit-lib";
import type { ArtifactDto, GalleryItemDto } from "@zvs/shared";
import {
  mdiApplicationOutline,
  mdiCheck,
  mdiDeleteOutline,
  mdiDotsHorizontal,
  mdiEyeOutline,
  mdiFolderOpenOutline,
  mdiOpenInNew,
  mdiRefresh,
  mdiShieldCheckOutline,
  mdiTimerOutline,
  mdiViewDashboardOutline,
  mdiWeb,
} from "@mdi/js";
import Icon from "../../ui/atoms/Icon";
import IconDropdownButton from "../../ui/atoms/buttons/IconDropdownButton";
import {
  ActionMenu,
  ActionMenuItem,
  ActionMenuSeparator,
  ActionSubmenu,
} from "../../ui/molecules/ActionMenu";
import ConfirmDialog from "../../ui/molecules/ConfirmDialog";
import { useStore } from "../../stores/useStore";
import { viewerCanShow } from "../../viewer/viewerModel";
import { TILE_BACKGROUNDS, formatIcon } from "../chat/ArtifactTile";
import {
  OPEN_IN_BUILT_IN_BROWSER,
  OPEN_IN_VIEWER,
  artifactKind,
  openInApplication,
  opensAsPage,
} from "../chat/artifacts";
import { STATUS_TONE_TEXT } from "../../ui/atoms/statusTone";
import type GalleryStore from "./GalleryStore";
import ScriptApprovalDialog from "./ScriptApprovalDialog";
import {
  GALLERY_ACTIONS,
  GALLERY_APPROVE,
  GALLERY_AUTO_REFRESH,
  GALLERY_AUTO_REFRESH_DEFAULT,
  GALLERY_INTERVALS,
  GALLERY_NEEDS_APPROVAL,
  GALLERY_OPEN,
  GALLERY_REFRESH,
  GALLERY_REMOVE,
  GALLERY_REMOVE_TITLE,
  GALLERY_REVEAL,
  intervalLabel,
  itemLine,
  removeQuestion,
} from "./galleryCopy";

function asArtifact(item: GalleryItemDto): ArtifactDto {
  return { id: item.id, name: item.name, format: item.format, sizeBytes: item.sizeBytes };
}

function useSeen<T extends Element>(): [React.RefObject<T | null>, boolean] {
  const ref = useRef<T | null>(null);
  const [seen, setSeen] = useState(false);
  useEffect(() => {
    const node = ref.current;
    if (node === null || seen) return;
    if (typeof IntersectionObserver !== "function") {
      setSeen(true);
      return;
    }
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) {
          setSeen(true);
          observer.disconnect();
        }
      },
      { rootMargin: "200px" },
    );
    observer.observe(node);
    return () => observer.disconnect();
  }, [seen]);
  return [ref, seen];
}

const Preview = observer(function Preview({
  store,
  item,
}: {
  readonly store: GalleryStore;
  readonly item: GalleryItemDto;
}) {
  const [ref, seen] = useSeen<HTMLDivElement>();
  useEffect(() => {
    if (seen) void store.loadThumbnail(item);
  }, [seen, store, item]);
  const thumbnail = store.thumbnailOf(item);
  const kind = artifactKind(item.format);
  const icon = item.kind === "dashboard" ? mdiViewDashboardOutline : formatIcon(item.format);
  return (
    <div ref={ref} className="relative min-h-0 flex-1 overflow-hidden bg-main-800">
      {thumbnail?.state === "ready" && thumbnail.url !== null ? (
        <img
          src={thumbnail.url}
          alt=""
          className="size-full object-cover object-top"
          draggable={false}
        />
      ) : (
        <div className="flex size-full items-center justify-center">
          <span
            className={`flex size-14 items-center justify-center rounded-card ${
              item.kind === "dashboard"
                ? "bg-accent-soft text-accent-medium"
                : `${TILE_BACKGROUNDS[kind.tone]} ${STATUS_TONE_TEXT[kind.tone]}`
            } ${thumbnail?.state === "loading" ? "anim-on:animate-pulse" : ""}`}
          >
            <Icon path={icon} size={26} />
          </span>
        </div>
      )}
    </div>
  );
});

const LiveBadge = observer(function LiveBadge({
  store,
  item,
}: {
  readonly store: GalleryStore;
  readonly item: GalleryItemDto;
}) {
  const status = store.statusOf(item.id);
  if (!item.live || status === undefined) return null;
  const waiting = store.needsApproval(item.id);
  const label = waiting ? GALLERY_NEEDS_APPROVAL : intervalLabel(status.interval);
  return (
    <span
      className={`absolute top-2 left-2 inline-flex h-5.5 items-center gap-1 rounded-[6px] px-1.75 text-[11px] font-medium shadow-card ${
        waiting ? "bg-warn-soft text-warn" : "bg-main-900/92 text-main-300"
      }`}
    >
      <Icon path={waiting ? mdiShieldCheckOutline : mdiTimerOutline} size={13} />
      {label}
    </span>
  );
});

const CardMenu = observer(function CardMenu({
  store,
  item,
  onRemove,
  onApprove,
}: {
  readonly store: GalleryStore;
  readonly item: GalleryItemDto;
  readonly onRemove: () => void;
  readonly onApprove: () => void;
}) {
  const { viewer } = useStore();
  const actions = store.actions;
  const page = opensAsPage(item.format);
  const viewable = viewerCanShow(item.format);
  const status = store.statusOf(item.id);
  const busy = store.pending.has(item.id);
  return (
    <Dropdown
      menuWidth={240}
      menuPlacement="bottom-right"
      onOpenChange={(open) => {
        if (!open) return;
        void actions.loadApplication(item.id, item.format);
        if (page) void actions.loadBrowsers();
        if (item.live) void store.loadStatus(item.id);
      }}
    >
      <IconDropdownButton
        label={GALLERY_ACTIONS}
        tooltip="top-center"
        path={mdiDotsHorizontal}
        size="sm"
        toneClassName="bg-main-900/92 text-main-300 shadow-card hover:bg-main-700 hover:text-main-50"
      />
      <ActionMenu label={GALLERY_ACTIONS}>
        <ActionSubmenu icon={mdiOpenInNew} label={GALLERY_OPEN}>
          {page ? (
            <ActionMenuItem
              icon={mdiApplicationOutline}
              onClick={() => void actions.openInApp(item.id)}
            >
              {OPEN_IN_BUILT_IN_BROWSER}
            </ActionMenuItem>
          ) : null}
          {page
            ? actions.browsersFor(item.format).map((browser) => (
                <ActionMenuItem
                  key={browser.id}
                  icon={mdiWeb}
                  onClick={() => void actions.openInBrowser(item.id, browser.id)}
                >
                  {openInApplication(browser.name)}
                </ActionMenuItem>
              ))
            : null}
          <ActionMenuItem icon={mdiOpenInNew} onClick={() => void actions.open(item.id)}>
            {openInApplication(actions.applicationOf(item.format))}
          </ActionMenuItem>
          {viewable ? (
            <ActionMenuItem
              icon={mdiEyeOutline}
              onClick={() => void viewer.openArtifact(asArtifact(item))}
            >
              {OPEN_IN_VIEWER}
            </ActionMenuItem>
          ) : null}
        </ActionSubmenu>
        <ActionMenuItem icon={mdiFolderOpenOutline} onClick={() => void actions.reveal(item.id)}>
          {GALLERY_REVEAL}
        </ActionMenuItem>
        {item.live && status !== undefined ? (
          <>
            <ActionMenuSeparator />
            <ActionMenuItem
              icon={mdiRefresh}
              disabled={busy || store.needsApproval(item.id)}
              onClick={() => void store.refresh(item.id)}
            >
              {GALLERY_REFRESH}
            </ActionMenuItem>
            <ActionSubmenu icon={mdiTimerOutline} label={GALLERY_AUTO_REFRESH} disabled={busy}>
              {GALLERY_INTERVALS.map((seconds) => (
                <ActionMenuItem
                  key={seconds}
                  icon={
                    status.overridden && status.interval === seconds ? mdiCheck : mdiTimerOutline
                  }
                  onClick={() => void store.setInterval(item.id, seconds)}
                >
                  {intervalLabel(seconds)}
                </ActionMenuItem>
              ))}
              <ActionMenuItem
                icon={status.overridden ? mdiTimerOutline : mdiCheck}
                onClick={() => void store.setInterval(item.id, null)}
              >
                {`${GALLERY_AUTO_REFRESH_DEFAULT}: ${intervalLabel(status.specInterval).toLocaleLowerCase("ru-RU")}`}
              </ActionMenuItem>
            </ActionSubmenu>
            {status.scripts.length > 0 ? (
              <ActionMenuItem icon={mdiShieldCheckOutline} onClick={onApprove}>
                {GALLERY_APPROVE}
              </ActionMenuItem>
            ) : null}
          </>
        ) : null}
        <ActionMenuSeparator />
        <ActionMenuItem icon={mdiDeleteOutline} danger disabled={busy} onClick={onRemove}>
          {GALLERY_REMOVE}
        </ActionMenuItem>
      </ActionMenu>
    </Dropdown>
  );
});

function GalleryCard({
  store,
  item,
}: {
  readonly store: GalleryStore;
  readonly item: GalleryItemDto;
}) {
  const { viewer } = useStore();
  const [removing, setRemoving] = useState(false);
  const [approving, setApproving] = useState(false);
  const owner = store.ownerLabel(item);
  const failure = store.failures.get(item.id) ?? store.actions.errors[item.id];
  const status = store.statusOf(item.id);
  const open = () => {
    if (opensAsPage(item.format) && item.kind === "dashboard")
      void store.actions.openInApp(item.id);
    else if (viewerCanShow(item.format)) void viewer.openArtifact(asArtifact(item));
    else void store.actions.open(item.id);
  };
  return (
    <li className="group relative flex aspect-square min-w-0 flex-col overflow-hidden rounded-card border border-line bg-main-850 shadow-card hover:border-line-strong">
      <button
        type="button"
        aria-label={`${GALLERY_OPEN}: ${item.name}`}
        className="flex min-h-0 flex-1 flex-col text-left"
        onClick={open}
      >
        <Preview store={store} item={item} />
      </button>
      <div className="flex flex-none flex-col gap-0.5 border-t border-line px-3 py-2.5">
        <span className="truncate text-[13px] font-semibold text-main-50" title={item.name}>
          {item.name}
        </span>
        <span className="truncate text-[11.5px] text-main-500">{itemLine(item, store.now)}</span>
        {owner === null ? null : (
          <span className="truncate text-[11.5px] text-main-400" title={owner}>
            {owner}
          </span>
        )}
        {failure === undefined ? null : (
          <span role="alert" className="line-clamp-2 text-[11.5px] text-err" title={failure}>
            {failure}
          </span>
        )}
      </div>
      <LiveBadge store={store} item={item} />
      <div className="absolute top-2 right-2">
        <CardMenu
          store={store}
          item={item}
          onRemove={() => setRemoving(true)}
          onApprove={() => setApproving(true)}
        />
      </div>
      <ConfirmDialog
        open={removing}
        title={GALLERY_REMOVE_TITLE}
        confirmLabel={GALLERY_REMOVE}
        confirmTone="danger"
        onCancel={() => setRemoving(false)}
        onConfirm={() => {
          setRemoving(false);
          void store.remove(item.id);
        }}
      >
        {removeQuestion(item.name, item.kind === "dashboard")}
      </ConfirmDialog>
      {status === undefined ? null : (
        <ScriptApprovalDialog
          open={approving}
          status={status}
          onCancel={() => setApproving(false)}
          onConfirm={() => {
            setApproving(false);
            void store.approve(item.id);
          }}
        />
      )}
    </li>
  );
}

export default observer(GalleryCard);
