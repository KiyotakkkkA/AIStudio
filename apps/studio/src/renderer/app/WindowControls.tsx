import { useCallback, useEffect, useState, type ReactNode } from "react";
import { WINDOW_CLOSE, WINDOW_MAXIMIZE, WINDOW_MINIMIZE, WINDOW_RESTORE } from "./titleBarCopy";
import { readMaximized, type WindowChannel } from "./windowFrame";

export function useWindowFrame() {
  const [maximized, setMaximized] = useState(false);

  const send = useCallback((channel: WindowChannel) => {
    void window.zvs
      .call(channel, undefined)
      .then((response) => {
        const next = readMaximized(response);
        if (next !== undefined) setMaximized(next);
      })
      .catch(() => undefined);
  }, []);

  useEffect(() => {
    const sync = () => send("window.state");
    sync();
    window.addEventListener("resize", sync);
    return () => window.removeEventListener("resize", sync);
  }, [send]);

  return { maximized, send };
}

const GLYPH = "size-2.5 fill-none stroke-current";

function Glyph({ children }: { readonly children: ReactNode }) {
  return (
    <svg viewBox="0 0 10 10" className={GLYPH} strokeWidth={1} aria-hidden="true">
      {children}
    </svg>
  );
}

const CONTROL =
  "flex h-full w-11.5 flex-none items-center justify-center text-main-400 transition-colors duration-100 hover:text-main-50";

export default function WindowControls({
  maximized,
  send,
}: {
  readonly maximized: boolean;
  readonly send: (channel: WindowChannel) => void;
}) {
  const resize = maximized ? WINDOW_RESTORE : WINDOW_MAXIMIZE;
  return (
    <div className="flex h-full flex-none items-stretch">
      <button
        type="button"
        aria-label={WINDOW_MINIMIZE}
        title={WINDOW_MINIMIZE}
        className={`${CONTROL} hover:bg-main-800`}
        onClick={() => send("window.minimize")}
      >
        <Glyph>
          <path d="M0 5.5h10" />
        </Glyph>
      </button>
      <button
        type="button"
        aria-label={resize}
        title={resize}
        className={`${CONTROL} hover:bg-main-800`}
        onClick={() => send("window.toggleMaximize")}
      >
        <Glyph>
          {maximized ? (
            <>
              <path d="M0.5 2.5h7v7h-7z" />
              <path d="M2.5 2.5v-2h7v7h-2" />
            </>
          ) : (
            <path d="M0.5 0.5h9v9h-9z" />
          )}
        </Glyph>
      </button>
      <button
        type="button"
        aria-label={WINDOW_CLOSE}
        title={WINDOW_CLOSE}
        className={`${CONTROL} hover:bg-[#c42b1c] hover:text-white`}
        onClick={() => send("window.close")}
      >
        <Glyph>
          <path d="M0.5 0.5l9 9M9.5 0.5l-9 9" />
        </Glyph>
      </button>
    </div>
  );
}
