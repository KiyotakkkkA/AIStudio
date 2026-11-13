export type WindowChannel =
  "window.state" | "window.minimize" | "window.toggleMaximize" | "window.close" | "window.drag";

export function readMaximized(response: unknown): boolean | undefined {
  if (typeof response !== "object" || response === null) return undefined;
  const maximized = (response as { maximized?: unknown }).maximized;
  return typeof maximized === "boolean" ? maximized : undefined;
}
