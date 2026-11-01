import "./app/tauriBridge";
import { createRoot } from "react-dom/client";
import { createEventRouter } from "./app/EventRouter";
import { bindAppearance } from "./app/appearance";
import { createReplayHook } from "./app/replay";
import Root from "./app/root";
import "./app/theme.css";
import { ipc } from "./ipc";
import { createRootStore } from "./stores/RootStore";

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

const store = createRootStore({ ipc, events });
const disposeAppearance = bindAppearance(store.ui);
const reactRoot = createRoot(container);
void store.boot().finally(() => {
  reactRoot.render(<Root store={store} />);
});

if (import.meta.hot) {
  import.meta.hot.accept();
  import.meta.hot.dispose(() => {
    reactRoot.unmount();
    disposeAppearance();
    store.dispose();
    events.dispose();
  });
}
