import { useEffect, useRef } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import "@xterm/xterm/css/xterm.css";
import "./terminal.css";
import type { TerminalId } from "@zvs/shared";
import type TerminalStore from "./TerminalStore";
import { adoptInlineStyles, readTheme, watchTheme } from "./terminalTheme";

const SYMBOLS_FONT = "Symbols Nerd Font Mono";
const FONT_SIZE = 12.5;
const FONT_FAMILY = `"JetBrains Mono Variable", "JetBrains Mono", "${SYMBOLS_FONT}", Consolas, monospace`;
const SYMBOL_SAMPLE = String.fromCodePoint(0xe0b0, 0xf120);

export function copySelection(terminal: Terminal): boolean {
  if (!terminal.hasSelection()) return false;
  void navigator.clipboard.writeText(terminal.getSelection()).catch(() => undefined);
  terminal.clearSelection();
  return true;
}

function loadWebgl(terminal: Terminal): boolean {
  try {
    const webgl = new WebglAddon();
    webgl.onContextLoss(() => webgl.dispose());
    terminal.loadAddon(webgl);
    return true;
  } catch {
    return false;
  }
}

export default function TerminalView({
  store,
  id,
  onReady,
}: {
  readonly store: TerminalStore;
  readonly id: TerminalId;
  readonly onReady?: (terminal: Terminal | null) => void;
}) {
  const host = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const element = host.current;
    if (element === null) return;
    const releaseStyles = adoptInlineStyles(element);
    const terminal = new Terminal({
      fontFamily: FONT_FAMILY,
      fontSize: FONT_SIZE,
      lineHeight: 1.2,
      cursorBlink: true,
      scrollback: 5000,
      theme: readTheme(),
    });
    const fit = new FitAddon();
    terminal.loadAddon(fit);
    terminal.open(element);
    loadWebgl(terminal);
    let disposed = false;
    void document.fonts
      .load(`${String(FONT_SIZE)}px "${SYMBOLS_FONT}"`, SYMBOL_SAMPLE)
      .then(() => {
        if (disposed) return;
        terminal.clearTextureAtlas();
        terminal.refresh(0, terminal.rows - 1);
      })
      .catch(() => undefined);
    terminal.attachCustomKeyEventHandler((event) => {
      if (event.type !== "keydown" || !event.ctrlKey || event.altKey) return true;
      const key = event.key.toLowerCase();
      if (key === "c") {
        const copied = copySelection(terminal);
        return !event.shiftKey && !copied;
      }
      return key !== "v";
    });
    let replaying = true;
    const input = terminal.onData((data) => {
      if (!replaying) store.write(id, data);
    });
    const resized = terminal.onResize(({ cols, rows }) => store.resize(id, cols, rows));
    const disconnect = store.connect(id, {
      write: (text) => terminal.write(text),
      reset: () => terminal.reset(),
    });
    terminal.write("", () => {
      replaying = false;
    });
    const refit = () => {
      if (element.clientWidth === 0 || element.clientHeight === 0) return;
      fit.fit();
      store.resize(id, terminal.cols, terminal.rows);
    };
    let frame = 0;
    const observer = new ResizeObserver(() => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(refit);
    });
    observer.observe(element);
    refit();
    const stopTheme = watchTheme(() => {
      terminal.options.theme = readTheme();
    });
    terminal.focus();
    onReady?.(terminal);
    return () => {
      disposed = true;
      onReady?.(null);
      cancelAnimationFrame(frame);
      observer.disconnect();
      stopTheme();
      disconnect();
      input.dispose();
      resized.dispose();
      terminal.dispose();
      releaseStyles();
    };
  }, [store, id, onReady]);

  return <div ref={host} className="size-full min-h-0 min-w-0 overflow-hidden px-3 pt-2" />;
}
