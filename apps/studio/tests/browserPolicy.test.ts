import { describe, expect, it } from "vitest";
import { BrowserCommand } from "@zvs/shared";
import { isWebUrl, navigationUrl } from "../src/host/browser/policy.ts";

describe("browser navigation policy", () => {
  it("normalizes typed sites and preserves local HTTP addresses", () => {
    expect(navigationUrl(" example.com/path ")).toBe("https://example.com/path");
    expect(navigationUrl("http://localhost:8123/a")).toBe("http://localhost:8123/a");
    expect(navigationUrl("localhost:8123/a")).toBe("https://localhost:8123/a");
  });
  it.each([
    "file:///etc/passwd",
    "javascript:alert(1)",
    "data:text/html,test",
    "about:blank",
    "ftp://example.com",
    "https://user:password@example.com",
    "not a url",
    "",
  ])("rejects %s", (url) => {
    expect(isWebUrl(url)).toBe(false);
    expect(() => navigationUrl(url)).toThrow();
  });
  it("rejects unknown commands and malformed mutation payloads", () => {
    expect(BrowserCommand.safeParse({ action: "execute", script: "alert(1)" }).success).toBe(false);
    expect(BrowserCommand.safeParse({ action: "clearSite", domain: "" }).success).toBe(false);
    expect(
      BrowserCommand.safeParse({ action: "reorder", id: "not-a-tab", index: -1 }).success,
    ).toBe(false);
  });
});
