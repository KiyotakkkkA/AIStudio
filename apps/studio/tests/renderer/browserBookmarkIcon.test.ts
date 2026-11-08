import { describe, expect, it, vi } from "vitest";
import { loadBookmarkIcon } from "../../src/browser-ui/loadBookmarkIcon";

describe("loadBookmarkIcon", () => {
  it("requests the saved bookmark by id and decodes the host image", async () => {
    const call = vi.fn().mockResolvedValue({ mime: "image/x-icon", data: "AAABAA==" });
    const blob = await loadBookmarkIcon("bookmark-id", call);
    expect(call).toHaveBeenCalledWith("browser.command", {
      action: "bookmarkIcon",
      id: "bookmark-id",
    });
    expect(blob?.type).toBe("image/x-icon");
    expect(blob?.size).toBe(4);
  });

  it("falls back when the icon is missing, invalid or unavailable", async () => {
    for (const response of [
      null,
      {},
      { mime: "image/svg+xml", data: "PHN2Zz4=" },
      { mime: "image/png", data: "!" },
    ]) {
      expect(await loadBookmarkIcon("bookmark-id", vi.fn().mockResolvedValue(response))).toBeNull();
    }
    expect(
      await loadBookmarkIcon("bookmark-id", vi.fn().mockRejectedValue(new Error("offline"))),
    ).toBeNull();
  });
});
