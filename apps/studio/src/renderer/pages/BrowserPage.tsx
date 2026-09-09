import { useLayoutEffect, useRef, useState } from "react";

export default function BrowserPage() {
  const panel = useRef<HTMLDivElement>(null);
  const [error, setError] = useState(false);
  useLayoutEffect(() => {
    const element = panel.current;
    if (!element) return;
    let disposed = false;
    const update = () => {
      const { x, y, width, height } = element.getBoundingClientRect();
      void window.zvs.call("browser.mount", { x, y, width, height }).catch(() => {
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
  }, []);
  return (
    <div ref={panel} className="relative min-h-0 flex-1" aria-label="Browser workspace">
      {error && (
        <p role="alert" className="p-5 text-err">
          Не удалось открыть браузер. Перейдите на другую страницу и попробуйте снова.
        </p>
      )}
    </div>
  );
}
