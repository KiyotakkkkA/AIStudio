import type { PointerEvent } from "react";
import { observer } from "mobx-react-lite";
import BrowserSurface from "../browser/BrowserSurface";
import { BROWSER_PANEL, RESIZE_BROWSER } from "../browser/browserCopy";
import { THREAD_MIN_WIDTH } from "../../viewer/viewerModel";
import type ChatStore from "./ChatStore";

export default observer(function ChatBrowserDock({ store }: { readonly store: ChatStore }) {
  const startResize = (event: PointerEvent<HTMLDivElement>) => {
    const handle = event.currentTarget;
    const dock = handle.parentElement;
    if (dock === null) return;
    handle.setPointerCapture(event.pointerId);
    const right = dock.getBoundingClientRect().right;
    const thread = dock.previousElementSibling?.getBoundingClientRect();
    const limit = thread === undefined ? Infinity : right - thread.left - THREAD_MIN_WIDTH;
    const move = (next: globalThis.PointerEvent) => {
      store.setBrowserWidth(Math.min(limit, right - next.clientX));
    };
    const stop = () => {
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", stop);
      handle.removeEventListener("pointercancel", stop);
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", stop);
    handle.addEventListener("pointercancel", stop);
  };
  return (
    <section
      aria-label={BROWSER_PANEL}
      style={{ flexBasis: `${String(store.browserWidth)}px` }}
      className="flex min-h-0 min-w-0 shrink grow-0 bg-main-900"
    >
      <div
        role="separator"
        aria-orientation="vertical"
        aria-label={RESIZE_BROWSER}
        onPointerDown={startResize}
        className="w-1.5 flex-none cursor-col-resize bg-main-800 transition-colors hover:bg-accent-line"
      />
      <BrowserSurface suspended={store.folderRequest !== null} />
    </section>
  );
});
