import { StrictMode, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { ipc, isIpcError } from "./ipc";

function Placeholder() {
  const [status, setStatus] = useState("Проверка связи с хостом…");

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

  return (
    <main>
      ZVS AI Studio
      <p>{status}</p>
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
