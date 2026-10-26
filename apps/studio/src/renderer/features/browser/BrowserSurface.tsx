import { observer } from "mobx-react-lite";
import { useLayoutEffect, useRef, useState } from "react";
import { interfaceZoom } from "../preferences/preferencesPresentation";
import { useStore } from "../../stores/useStore";
import { BROWSER_AREA, BROWSER_MOUNT_FAILED } from "./browserCopy";

export default observer(function BrowserSurface({
  suspended = false,
  className = "",
}: {
  readonly suspended?: boolean;
  readonly className?: string;
}) {
  const { ui } = useStore();
  const zoom = interfaceZoom(ui.fontSize);
  const hidden = suspended || ui.preferences !== null;
  const panel = useRef<HTMLDivElement>(null);
  const [error, setError] = useState(false);
  useLayoutEffect(() => {
    const element = panel.current;
    if (!element || hidden) return;
    let disposed = false;
    const update = () => {
      const rect = element.getBoundingClientRect();
      const region = {
        x: rect.x * zoom,
        y: rect.y * zoom,
        width: rect.width * zoom,
        height: rect.height * zoom,
      };
      void window.zvs.call("browser.mount", region).catch(() => {
        if (!disposed) setError(true);
      });
    };
    const observer = new ResizeObserver(update);
    observer.observe(element);
    window.addEventListener("resize", update);
    update();
    return () => {
      disposed = true;
      observer.disconnect();
      window.removeEventListener("resize", update);
      void window.zvs.call("browser.hide", undefined).catch(() => undefined);
    };
  }, [zoom, hidden]);
  return (
    <div ref={panel} className={`relative min-h-0 flex-1 ${className}`} aria-label={BROWSER_AREA}>
      {error && (
        <p role="alert" className="p-5 text-err">
          {BROWSER_MOUNT_FAILED}
        </p>
      )}
    </div>
  );
});
