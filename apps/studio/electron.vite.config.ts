import { cpSync } from "node:fs";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "electron-vite";

const MIGRATIONS_SOURCE = "src/host/data/migrations";
const MIGRATIONS_OUTPUT = "out/host/migrations";

const copyMigrations = () => ({
  name: "copy-migrations",
  closeBundle() {
    cpSync(MIGRATIONS_SOURCE, MIGRATIONS_OUTPUT, { recursive: true });
  },
});

export default defineConfig({
  main: {
    plugins: [copyMigrations()],
    build: {
      outDir: "out/host",
      rollupOptions: {
        input: "src/host/main.ts",
        output: { entryFileNames: "main.js" },
        external: ["better-sqlite3"],
      },
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
    resolve: {
      dedupe: ["react", "react-dom"],
    },
    optimizeDeps: {
      include: [
        "react",
        "react-dom/client",
        "react/jsx-runtime",
        "react/jsx-dev-runtime",
        "mobx",
        "mobx-react-lite",
      ],
    },
    define: {
      __ZVS_VALIDATE_IPC__: JSON.stringify(process.env.ZVS_VALIDATE_IPC !== "false"),
    },
    plugins: [
      tailwindcss(),
      {
        name: "development-csp",
        apply: "serve",
        transformIndexHtml(html, context) {
          const address = context.server?.resolvedUrls?.local[0];
          if (!address) throw new Error("Development server URL is unavailable");
          const websocketOrigin = new URL(address).origin.replace(/^http/, "ws");
          return html
            .replace("connect-src 'none'", `connect-src 'self' ${websocketOrigin}`)
            .replace("style-src 'self'", "style-src 'self' 'unsafe-inline'");
        },
      },
    ],
    build: { rollupOptions: { input: "src/renderer/index.html" } },
  },
});
