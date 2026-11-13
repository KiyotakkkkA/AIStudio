import { describe, expect, it } from "vitest";
import {
  canGoBack,
  canGoForward,
  historyIndex,
  startPosition,
  stepPosition,
} from "../../src/renderer/app/historyPosition";
import { readMaximized } from "../../src/renderer/app/windowFrame";

describe("title bar history", () => {
  it("reads the router index from history state and ignores anything else", () => {
    expect(historyIndex({ idx: 3, key: "a" })).toBe(3);
    expect(historyIndex(null)).toBe(0);
    expect(historyIndex({ idx: -1 })).toBe(0);
    expect(historyIndex({ idx: "2" })).toBe(0);
  });

  it("starts with nothing to go back or forward to", () => {
    const start = startPosition(0);
    expect(canGoBack(start)).toBe(false);
    expect(canGoForward(start)).toBe(false);
  });

  it("enables forward after going back and keeps it until a new page is pushed", () => {
    let position = startPosition(0);
    position = stepPosition(position, "PUSH", 1);
    position = stepPosition(position, "PUSH", 2);
    expect(canGoBack(position)).toBe(true);
    expect(canGoForward(position)).toBe(false);

    position = stepPosition(position, "POP", 1);
    expect(canGoForward(position)).toBe(true);
    position = stepPosition(position, "POP", 0);
    expect(canGoBack(position)).toBe(false);
    expect(canGoForward(position)).toBe(true);

    position = stepPosition(position, "PUSH", 1);
    expect(canGoForward(position)).toBe(false);
  });

  it("does not open a forward step when a page is replaced", () => {
    const position = stepPosition(startPosition(2), "REPLACE", 2);
    expect(canGoForward(position)).toBe(false);
    expect(canGoBack(position)).toBe(true);
  });
});

describe("window frame state", () => {
  it("accepts only a boolean maximized flag from the host", () => {
    expect(readMaximized({ maximized: true })).toBe(true);
    expect(readMaximized({ maximized: false })).toBe(false);
    expect(readMaximized({ maximized: "yes" })).toBeUndefined();
    expect(readMaximized(null)).toBeUndefined();
  });
});
