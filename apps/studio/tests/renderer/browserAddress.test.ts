import { describe, expect, it } from "vitest";
import { displayAddress } from "../../src/browser-ui/address";

describe("displayAddress", () => {
  it("shows only the host unless the full address is requested", () => {
    const url = "https://docs.example.com:8443/guide/start?tab=2#install";
    expect(displayAddress(url, false)).toBe("docs.example.com:8443");
    expect(displayAddress(url, true)).toBe(url);
  });

  it("leaves typed text, blank pages and other schemes unchanged", () => {
    expect(displayAddress("", false)).toBe("");
    expect(displayAddress("example", false)).toBe("example");
    expect(displayAddress("about:blank", false)).toBe("about:blank");
  });
});
