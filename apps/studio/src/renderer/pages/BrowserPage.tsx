import { observer } from "mobx-react-lite";
import { useLayoutEffect, useRef, useState } from "react";
import { interfaceZoom } from "../features/preferences/preferencesPresentation";
import { useStore } from "../stores/useStore";

function BrowserPage() {
  const { ui } = useStore();
  const zoom = interfaceZoom(ui.fontSize);
  const panel = useRef<HTMLDivElement>(null);
  const [error, setError] = useState(false);
  useLayoutEffect(() => {
    const element = panel.current;
    if (!element) return;
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
  }, [zoom]);
  return (
    <div ref={panel} className="relative min-h-0 flex-1" aria-label="Рабочая область браузера">
      {error && (
        <p role="alert" className="p-5 text-err">
          Не удалось открыть браузер. Перейдите на другую страницу и попробуйте снова.
        </p>
      )}
    </div>
  );
}

export default observer(BrowserPage);
