import { randomBytes } from "node:crypto";
import { readFileSync } from "node:fs";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig, type Plugin } from "vite";

interface TauriConfig {
  build: { devUrl: string };
  app: { security: { devCsp: string } };
}

const tauriConfig = JSON.parse(
  readFileSync(new URL("./src-tauri/tauri.conf.json", import.meta.url), "utf8"),
) as TauriConfig;

const CSP_META = /\s*<meta\s+http-equiv="Content-Security-Policy"[^>]*>/i;

function developmentPolicy(nonce: string): string {
  return tauriConfig.app.security.devCsp
    .split(";")
    .map((directive) => directive.trim())
    .filter((directive) => directive.length > 0 && !directive.startsWith("frame-ancestors"))
    .map((directive) =>
      directive.startsWith("style-src ") ? `${directive} 'nonce-${nonce}'` : directive,
    )
    .join("; ");
}

function tauriContentSecurityPolicy(command: "build" | "serve", nonce: string): Plugin {
  return {
    name: "tauri-content-security-policy",
    transformIndexHtml: {
      order: "pre",
      handler(html) {
        if (!CSP_META.test(html)) throw new Error("Content-Security-Policy meta tag is missing");
        if (command === "build") return html.replace(CSP_META, "");
        return html.replace(
          CSP_META,
          `\n    <meta http-equiv="Content-Security-Policy" content="${developmentPolicy(nonce)}" />`,
        );
      },
    },
  };
}

export default defineConfig(({ command }) => {
  const nonce = randomBytes(16).toString("base64");
  const devPort = Number(new URL(tauriConfig.build.devUrl).port);
  return {
    root: "src",
    clearScreen: false,
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
    html: command === "serve" ? { cspNonce: nonce } : {},
    plugins: [tailwindcss(), tauriContentSecurityPolicy(command, nonce)],
    server: {
      port: devPort,
      strictPort: true,
      host: "localhost",
    },
    build: {
      outDir: "../dist",
      emptyOutDir: true,
      assetsInlineLimit: 0,
      rollupOptions: { input: { studio: "src/index.html", browser: "src/browser-ui/index.html" } },
    },
  };
});
