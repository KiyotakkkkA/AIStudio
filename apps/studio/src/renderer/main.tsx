import { createRoot } from "react-dom/client";
import { createEventRouter } from "./app/EventRouter";
import { createReplayHook } from "./app/replay";
import Root from "./app/root";
import "./app/theme.css";
import { ipc } from "./ipc";

const events = createEventRouter();
events.connect((handler) => window.zvs.subscribe(handler));

if (import.meta.env.DEV) {
  Object.defineProperty(window, "zvsReplay", {
    value: createReplayHook(events),
    configurable: true,
  });
}

const container = document.getElementById("root");
if (!container) throw new Error("Renderer root is missing");

const reactRoot = createRoot(container);
reactRoot.render(<Root environment={{ ipc, events }} />);

if (import.meta.hot) {
  import.meta.hot.accept();
  import.meta.hot.dispose(() => {
    events.dispose();
    reactRoot.unmount();
  });
}
