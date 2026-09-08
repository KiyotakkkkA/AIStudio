import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

const root = document.getElementById("root");
if (!root) throw new Error("Renderer root is missing");
const reactRoot = createRoot(root);
reactRoot.render(
  <StrictMode>
    <main>ZVS AI Studio</main>
  </StrictMode>,
);

if (import.meta.hot) {
  import.meta.hot.accept();
  import.meta.hot.dispose(() => reactRoot.unmount());
}
