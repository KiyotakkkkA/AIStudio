import { describe, expect, it } from "vitest";
import type { BrowserNetworkEntry } from "@zvs/shared";
import {
  entryName,
  formatDuration,
  formatSize,
  matches,
  pretty,
  requestsWord,
  transferred,
  typeGroup,
} from "../../src/browser-ui/networkCopy";

function entry(overrides: Partial<BrowserNetworkEntry>): BrowserNetworkEntry {
  return {
    id: "1",
    url: "https://a.example/api/items?page=2",
    method: "GET",
    resourceType: "Fetch",
    state: "finished",
    startedAt: 0,
    fromCache: false,
    ...overrides,
  };
}

describe("network panel helpers", () => {
  it("groups CDP resource types the way the filter chips do", () => {
    expect(typeGroup("XHR")).toBe("fetch");
    expect(typeGroup("Fetch")).toBe("fetch");
    expect(typeGroup("Stylesheet")).toBe("style");
    expect(typeGroup("Ping")).toBe("other");
  });

  it("filters by type and by a case-insensitive address fragment", () => {
    const fetch = entry({});
    const script = entry({ id: "2", url: "https://cdn.example/app.js", resourceType: "Script" });
    expect([fetch, script].filter((item) => matches(item, "script", ""))).toEqual([script]);
    expect([fetch, script].filter((item) => matches(item, "all", " ITEMS "))).toEqual([fetch]);
  });

  it("names a request by its last path segment and falls back to the host", () => {
    expect(entryName("https://a.example/api/items?page=2")).toEqual({
      name: "items?page=2",
      host: "a.example",
    });
    expect(entryName("https://a.example/")).toEqual({ name: "a.example", host: "a.example" });
    expect(entryName("not a url")).toEqual({ name: "not a url", host: "" });
  });

  it("formats sizes, durations and counts in the ru-RU locale", () => {
    expect(formatSize(undefined)).toBe("—");
    expect(formatSize(512)).toBe("512 Б");
    expect(formatSize(1536)).toBe(`${(1.5).toLocaleString("ru-RU")} КБ`);
    expect(formatDuration(249.6)).toBe("250 мс");
    expect(formatDuration(1250)).toBe(`${(1.25).toLocaleString("ru-RU")} с`);
    expect(requestsWord(3)).toBe("3 запроса");
    expect(
      transferred([entry({ size: 10 }), entry({ id: "2" }), entry({ id: "3", size: 5 })]),
    ).toBe(15);
  });

  it("pretty-prints JSON and leaves other text alone", () => {
    expect(pretty('{"a":1}')).toBe('{\n  "a": 1\n}');
    expect(pretty("plain")).toBe("plain");
  });
});
