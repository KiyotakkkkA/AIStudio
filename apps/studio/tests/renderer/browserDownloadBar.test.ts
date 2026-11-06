import { describe, expect, it } from "vitest";
import {
  DOWNLOAD_LOADING,
  downloadDetail,
  downloadSize,
  downloadTitle,
} from "../../src/browser-ui/downloadCopy";

const download = {
  id: "0199aa22-2222-7222-8222-222222222222",
  fileName: "report.pdf",
  host: "files.example",
  bytes: 1_536,
  ready: true,
};

describe("download bar copy", () => {
  it("names the site and the file", () => {
    expect(downloadTitle(download)).toBe("files.example загружает файл «report.pdf»");
    expect(downloadTitle({ ...download, host: "" })).toBe("Сайт загружает файл «report.pdf»");
  });

  it("shows the size once the file is ready and counts the queue", () => {
    expect(downloadSize(512)).toBe("512 Б");
    expect(downloadSize(1_536)).toBe("1,5 КБ");
    expect(downloadDetail(download, 0)).toBe("1,5 КБ");
    expect(downloadDetail({ ...download, ready: false }, 2)).toBe(
      `${DOWNLOAD_LOADING} · ещё в очереди: 2`,
    );
  });
});
