import { expect, test } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  AppErrorCode,
  contract,
  type Contract,
  type DashboardStatusDto,
  type GalleryItemDto,
  type GalleryThumbnailDto,
  ConversationDto,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import GalleryStore, { type ObjectUrls } from "../../src/renderer/features/gallery/GalleryStore.ts";
import {
  GALLERY_LOAD_FAILED,
  GALLERY_OUTSIDE_DIALOG,
  intervalLabel,
  removeQuestion,
} from "../../src/renderer/features/gallery/galleryCopy.ts";

const HASH = "3f".repeat(32);

function conversation(id: string, title: string): ConversationDto {
  return ConversationDto.parse({
    id,
    title,
    providerId: "0199ff11-1111-7111-8111-000000000001",
    modelId: "m",
    createdAt: 1,
    updatedAt: 1,
  });
}

function item(overrides: Partial<GalleryItemDto>): GalleryItemDto {
  return {
    id: "dashboards/Продажи.html",
    name: "Продажи.html",
    format: "html",
    kind: "dashboard",
    sizeBytes: 1024,
    modifiedAt: 1_000,
    owner: "dashboards",
    live: false,
    ...overrides,
  };
}

const ITEMS: GalleryItemDto[] = [
  item({ live: true }),
  item({
    id: "0199aa11-2222-7111-8111-111111111111/Отчёт.pdf",
    name: "Отчёт.pdf",
    format: "pdf",
    kind: "document",
    owner: "0199aa11-2222-7111-8111-111111111111",
    modifiedAt: 3_000,
  }),
  item({
    id: "0199aa11-2222-7111-8111-222222222222/chart.png",
    name: "chart.png",
    format: "png",
    kind: "image",
    owner: "0199aa11-2222-7111-8111-222222222222",
    modifiedAt: 2_000,
  }),
  item({
    id: "run-9/Архив.docx",
    name: "Архив.docx",
    format: "docx",
    kind: "document",
    owner: "run-9",
    modifiedAt: 500,
  }),
];

function status(overrides: Partial<DashboardStatusDto> = {}): DashboardStatusDto {
  return {
    id: "dashboards/Продажи.html",
    live: true,
    interval: 60,
    specInterval: 60,
    overridden: false,
    refreshing: false,
    scripts: [
      { name: "1-crm.py", sha256: HASH, approved: false, source: "print(1)", truncated: false },
    ],
    ...overrides,
  };
}

function urls(): ObjectUrls & { created: string[]; revoked: string[] } {
  const created: string[] = [];
  const revoked: string[] = [];
  return {
    created,
    revoked,
    create: () => {
      const url = `blob:${created.length}`;
      created.push(url);
      return url;
    },
    revoke: (url) => {
      revoked.push(url);
    },
  };
}

function setup(items: GalleryItemDto[] = ITEMS) {
  const objectUrls = urls();
  const bridge = createFakeBridge<Contract>()
    .handle("gallery.list", () => ({ items }))
    .handle("chat.conversations.list", () => [
      conversation("0199aa11-2222-7111-8111-111111111111", "Квартальный отчёт"),
      conversation("0199aa11-2222-7111-8111-222222222222", "Графики"),
    ])
    .handle("dashboards.status", () => status());
  const store = new GalleryStore(createIpcClient(contract, bridge), {
    urls: objectUrls,
    now: () => 10_000,
    renderPdf: async () => new Blob(["png"], { type: "image/png" }),
  });
  return { bridge, store, objectUrls };
}

test("files are grouped by kind in a fixed order and empty groups are left out", async () => {
  const { store } = setup();
  await store.load();
  expect(store.sections.map((section) => section.kind)).toEqual(["dashboard", "document", "image"]);
  expect(store.sections[1]?.items.map((entry) => entry.name)).toEqual(["Отчёт.pdf", "Архив.docx"]);
  expect(store.statusOf("dashboards/Продажи.html")?.interval).toBe(60);
  expect(store.needsApproval("dashboards/Продажи.html")).toBe(true);
});

test("search matches the file name or the dialog title, and sorting reorders inside a group", async () => {
  const { store } = setup();
  await store.load();
  store.setQuery("квартальный");
  expect(store.visible.map((entry) => entry.id)).toEqual([
    "0199aa11-2222-7111-8111-111111111111/Отчёт.pdf",
  ]);
  store.setQuery("");
  store.setSort("oldest");
  expect(store.sections[1]?.items.map((entry) => entry.name)).toEqual(["Архив.docx", "Отчёт.pdf"]);
  store.setSort("name");
  expect(store.visible.map((entry) => entry.name)).toEqual([
    "Архив.docx",
    "Отчёт.pdf",
    "Продажи.html",
    "chart.png",
  ]);
  store.setQuery("нет такого");
  expect(store.sections).toEqual([]);
  expect(store.isEmpty).toBe(false);
});

test("a dialog title names the owner, dashboards have none and unknown owners are outside a dialog", async () => {
  const { store } = setup();
  await store.load();
  expect(store.ownerLabel(ITEMS[0]!)).toBeNull();
  expect(store.ownerLabel(ITEMS[1]!)).toBe("Квартальный отчёт");
  expect(store.ownerLabel(ITEMS[3]!)).toBe(GALLERY_OUTSIDE_DIALOG);
});

test("previews load once per file version, render PDFs and are released when the file goes", async () => {
  const { bridge, store, objectUrls } = setup();
  bridge.handle("gallery.thumbnail", ({ id }): GalleryThumbnailDto =>
    id.endsWith(".pdf")
      ? { mediaType: "application/pdf" as const, data: btoa("%PDF") }
      : id.endsWith(".png")
        ? { mediaType: "image/png" as const, data: btoa("png") }
        : {},
  );
  bridge.handle("gallery.remove", () => undefined);
  await store.load();
  const [dashboard, pdf, png] = ITEMS;
  await Promise.all([
    store.loadThumbnail(dashboard!),
    store.loadThumbnail(pdf!),
    store.loadThumbnail(png!),
  ]);
  await store.loadThumbnail(png!);
  expect(bridge.calls.filter((call) => call.channel === "gallery.thumbnail")).toHaveLength(3);
  expect(store.thumbnailOf(dashboard!)).toEqual({ state: "none", url: null });
  expect(store.thumbnailOf(pdf!)?.state).toBe("ready");
  expect(store.thumbnailOf(png!)?.state).toBe("ready");
  expect(objectUrls.created).toHaveLength(2);

  expect(await store.remove(png!.id)).toBe(true);
  expect(store.items.map((entry) => entry.id)).not.toContain(png!.id);
  expect(objectUrls.revoked).toHaveLength(1);
  store.dispose();
  expect(objectUrls.revoked).toHaveLength(2);
});

test("approving sends exactly the current hashes and a refresh reloads the list", async () => {
  const { bridge, store } = setup();
  bridge
    .handle("dashboards.approve", ({ hashes }) => {
      expect(hashes).toEqual([HASH]);
      return status({
        scripts: [{ name: "1-crm.py", sha256: HASH, approved: true, source: "", truncated: false }],
      });
    })
    .handle("dashboards.refresh", () => status({ refreshedAt: 9_000 }))
    .handle("dashboards.setInterval", ({ interval }) =>
      status({ interval: interval ?? 60, overridden: interval !== undefined }),
    );
  await store.load();
  expect(await store.approve("dashboards/Продажи.html")).toBe(true);
  expect(store.needsApproval("dashboards/Продажи.html")).toBe(false);
  bridge.handle("dashboards.status", () =>
    status({
      refreshedAt: 9_000,
      scripts: [{ name: "1-crm.py", sha256: HASH, approved: true, source: "", truncated: false }],
    }),
  );
  expect(await store.refresh("dashboards/Продажи.html")).toBe(true);
  expect(bridge.calls.filter((call) => call.channel === "gallery.list")).toHaveLength(2);
  expect(await store.setInterval("dashboards/Продажи.html", 300)).toBe(true);
  expect(store.statusOf("dashboards/Продажи.html")?.interval).toBe(300);
  expect(await store.setInterval("dashboards/Продажи.html", null)).toBe(true);
  expect(bridge.calls.at(-1)?.payload).toEqual({ id: "dashboards/Продажи.html" });
});

test("a refused refresh keeps the message the host worded for the card", async () => {
  const { bridge, store } = setup();
  bridge.fail("dashboards.refresh", AppErrorCode.PERMISSION_DENIED, "Подтвердите скрипты");
  await store.load();
  expect(await store.refresh("dashboards/Продажи.html")).toBe(false);
  expect(store.failures.get("dashboards/Продажи.html")).toBe("Подтвердите скрипты");
  store.dismissFailure("dashboards/Продажи.html");
  expect(store.failures.size).toBe(0);
});

test("a failed list is reported and an empty one is empty", async () => {
  const failing = setup();
  failing.bridge.fail("gallery.list", AppErrorCode.UNKNOWN, "boom");
  await failing.store.load();
  expect(failing.store.error).toBe(GALLERY_LOAD_FAILED);
  expect(failing.store.isEmpty).toBe(false);
  const empty = setup([]);
  await empty.store.load();
  expect(empty.store.isEmpty).toBe(true);
});

test("intervals are named in whole units and removal names the file", () => {
  expect([0, 15, 60, 300, 3600, 7200].map(intervalLabel)).toEqual([
    "Вручную",
    "Каждые 15 с",
    "Каждую минуту",
    "Каждые 5 мин",
    "Каждый час",
    "Каждые 2 ч",
  ]);
  expect(removeQuestion("Продажи.html", true)).toContain("«Продажи.html»");
  expect(removeQuestion("Продажи.html", true)).not.toBe(removeQuestion("Продажи.html", false));
});
