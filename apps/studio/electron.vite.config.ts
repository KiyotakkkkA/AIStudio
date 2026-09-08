import { defineConfig } from "electron-vite";

export default defineConfig({
  main: {
    build: {
      outDir: "out/host",
      rollupOptions: { input: "src/host/main.ts", output: { entryFileNames: "main.js" } },
    },
  },
  preload: {
    build: {
      rollupOptions: {
        input: "src/preload/index.ts",
        output: { format: "cjs", entryFileNames: "index.cjs" },
      },
    },
  },
  renderer: {
    root: "src/renderer",
    define: {
      __ZVS_VALIDATE_IPC__: JSON.stringify(process.env.ZVS_VALIDATE_IPC !== "false"),
    },
    plugins: [
      {
        name: "development-csp",
        apply: "serve",
        transformIndexHtml(html, context) {
          const address = context.server?.resolvedUrls?.local[0];
          if (!address) throw new Error("Development server URL is unavailable");
          const websocketOrigin = new URL(address).origin.replace(/^http/, "ws");
          return html.replace("connect-src 'none'", `connect-src 'self' ${websocketOrigin}`);
        },
      },
    ],
    build: { rollupOptions: { input: "src/renderer/index.html" } },
  },
});
