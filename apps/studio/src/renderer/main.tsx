import { StrictMode, useCallback, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import type { StreamId } from "@zvs/shared";
import { createEventRouter } from "./app/EventRouter";
import { createReplayHook } from "./app/replay";
import { ipc, isIpcError } from "./ipc";

const DEMO_STEPS = 10;

interface DemoStream {
  id: StreamId;
  done: number;
  total: number;
  finished: boolean;
  gaps: number;
}

const router = createEventRouter();

if (import.meta.env.DEV) {
  Object.defineProperty(window, "zvsReplay", { value: createReplayHook(router) });
}

function Placeholder() {
  const [status, setStatus] = useState("Проверка связи с хостом…");
  const [geometry, setGeometry] = useState("Размер окна ещё не сохранён");
  const [streams, setStreams] = useState<DemoStream[]>([]);

  useEffect(() => {
    let active = true;
    const sentAt = Date.now();
    ipc
      .call("system.ping", { sentAt })
      .then((pong) => {
        if (!active) return;
        setStatus(`Ответ хоста за ${Date.now() - sentAt} мс (сдвиг ${pong.roundTripHint} мс)`);
      })
      .catch((error: unknown) => {
        if (!active) return;
        setStatus(isIpcError(error) ? `Ошибка связи: ${error.code}` : "Ошибка связи");
      });
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    let active = true;
    ipc
      .call("settings.get", { key: "window.main" })
      .then((stored) => {
        if (!active) return;
        const bounds = (
          stored.value as { bounds?: { width?: number; height?: number } } | undefined
        )?.bounds;
        if (bounds?.width === undefined || bounds.height === undefined) return;
        setGeometry(`Сохранённый размер окна: ${bounds.width} × ${bounds.height}`);
      })
      .catch(() => {
        if (active) setGeometry("Не удалось прочитать настройки");
      });
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => router.connect((handler) => window.zvs.subscribe(handler)), []);

  const start = useCallback(() => {
    void ipc
      .call("system.demoStream", { steps: DEMO_STEPS })
      .then(({ streamId }) => {
        setStreams((current) => [
          ...current,
          { id: streamId, done: 0, total: DEMO_STEPS, finished: false, gaps: 0 },
        ]);
        let ended = false;
        const dispose = router.subscribe(streamId, (event) => {
          setStreams((current) =>
            current.map((stream) => {
              if (stream.id !== streamId) return stream;
              const gaps = stream.gaps + (event.gap ?? 0);
              if (event.type === "progress")
                return { ...stream, done: event.done, total: event.total, gaps };
              if (event.type === "end") return { ...stream, finished: true, gaps };
              return { ...stream, gaps };
            }),
          );
          if (event.type === "end") {
            ended = true;
            dispose?.();
          }
        });
        if (ended) dispose();
      })
      .catch(() => {
        setStatus("Не удалось запустить поток");
      });
  }, []);

  return (
    <main>
      ZVS AI Studio
      <p>{status}</p>
      <p>{geometry}</p>
      <button type="button" onClick={start}>
        Запустить демонстрационный поток
      </button>
      <ul>
        {streams.map((stream, index) => (
          <li key={stream.id}>
            Поток {index + 1} ({stream.id.slice(-8)}): {stream.done} / {stream.total}
            {stream.finished ? " — завершён" : ""}
            {stream.gaps > 0 ? ` — пропущено ${stream.gaps}` : ""}
          </li>
        ))}
      </ul>
    </main>
  );
}

const root = document.getElementById("root");
if (!root) throw new Error("Renderer root is missing");
const reactRoot = createRoot(root);
reactRoot.render(
  <StrictMode>
    <Placeholder />
  </StrictMode>,
);

if (import.meta.hot) {
  import.meta.hot.accept();
  import.meta.hot.dispose(() => reactRoot.unmount());
}
