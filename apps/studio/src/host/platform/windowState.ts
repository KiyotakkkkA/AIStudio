export const WINDOW_STATE_KEY = "window.main";

interface WindowBounds {
  x?: number;
  y?: number;
  width: number;
  height: number;
}

export interface WindowState {
  bounds: WindowBounds;
  maximized: boolean;
}

export const MIN_WINDOW_WIDTH = 1100;
export const MIN_WINDOW_HEIGHT = 700;

export const DEFAULT_WINDOW_STATE: WindowState = {
  bounds: { width: 1440, height: 900 },
  maximized: true,
};

const isFinite = (value: unknown): value is number =>
  typeof value === "number" && Number.isFinite(value);

export function readWindowState(stored: unknown): WindowState {
  if (typeof stored !== "object" || stored === null) return DEFAULT_WINDOW_STATE;
  const { bounds, maximized } = stored as { bounds?: unknown; maximized?: unknown };
  if (typeof bounds !== "object" || bounds === null) return DEFAULT_WINDOW_STATE;
  const { x, y, width, height } = bounds as Record<string, unknown>;
  if (!isFinite(width) || !isFinite(height)) return DEFAULT_WINDOW_STATE;
  const positioned = isFinite(x) && isFinite(y);
  return {
    bounds: {
      ...(positioned ? { x, y } : {}),
      width: Math.max(MIN_WINDOW_WIDTH, Math.round(width)),
      height: Math.max(MIN_WINDOW_HEIGHT, Math.round(height)),
    },
    maximized: maximized === true,
  };
}

export interface WindowLike {
  isMaximized(): boolean;
  isMinimized(): boolean;
  isFullScreen(): boolean;
  getNormalBounds(): { x: number; y: number; width: number; height: number };
}

export function captureWindowState(window: WindowLike): WindowState {
  const { x, y, width, height } = window.getNormalBounds();
  return {
    bounds: { x, y, width, height },
    maximized: window.isMaximized() || window.isFullScreen(),
  };
}
