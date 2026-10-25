import { expect, test } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import { AppErrorCode, ChatToolCallDto, contract, type Contract } from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import ArtifactActions from "../../src/renderer/features/chat/ArtifactActions.ts";
import { formatBytes } from "../../src/renderer/features/downloads/downloadPresentation.ts";
import {
  ARTIFACT_ERROR_COPY,
  ARTIFACT_SAVED,
  DASHBOARD_KIND,
  answerArtifacts,
  artifactKind,
  artifactSubtitle,
  opensAsPage,
  stepArtifacts,
} from "../../src/renderer/features/chat/artifacts.ts";

const RUN = "0199aa11-4444-7444-8444-444444444444";

const artifact = (name: string, sizeBytes = 2048) => ({
  id: `${RUN}/${name}`,
  name,
  format: name.split(".").at(-1) ?? "",
  sizeBytes,
});

const call = (status: string, meta: unknown) =>
  ChatToolCallDto.parse({
    id: "call_1",
    key: "skill.script",
    name: "skill__script",
    round: 0,
    arguments: { key: "docx", path: "scripts/build.py" },
    status,
    meta,
  });

test("files a skill script reports become the answer's artifacts, the latest copy winning", () => {
  const listed = answerArtifacts([
    call("succeeded", { artifacts: [artifact("Отчёт.docx", 10), artifact("data.xlsx")] }),
    call("failed", { artifacts: [artifact("lost.pdf")] }),
    call("succeeded", { artifacts: "broken" }),
    call("succeeded", { artifacts: [{ ...artifact("x.pdf"), id: "../escape.pdf" }] }),
    call("succeeded", { artifacts: [artifact("Отчёт.docx", 30)] }),
  ]);
  expect(listed.map((row) => [row.name, row.sizeBytes])).toEqual([
    ["data.xlsx", 2048],
    ["Отчёт.docx", 30],
  ]);
  expect(answerArtifacts([])).toEqual([]);
});

test("an artifact names its kind, format and size", () => {
  expect(artifactSubtitle(artifact("Отчёт.docx"))).toBe(
    `Документ Word · DOCX · ${formatBytes(2048)}`,
  );
  expect(artifactKind("PDF").tone).toBe("err");
  expect(artifactKind("pptx").label).toBe("Презентация PowerPoint");
  expect(artifactKind("bin")).toEqual({ label: "Файл", tone: "idle" });
});

function actions() {
  const bridge = createFakeBridge<Contract>();
  return { bridge, store: new ArtifactActions(createIpcClient(contract, bridge)) };
}

test("download saves through the host and reports only a completed save", async () => {
  const { bridge, store } = actions();
  const id = `${RUN}/Отчёт.docx`;
  bridge.handle("artifacts.save", () => ({}));
  expect(await store.save(id)).toBe(true);
  expect(store.notices[id]).toBeUndefined();
  bridge.handle("artifacts.save", () => ({ path: "C:/Users/a/Отчёт.docx" }));
  expect(await store.save(id)).toBe(true);
  expect(store.notices[id]).toBe(ARTIFACT_SAVED);
  expect(store.busy[id]).toBeUndefined();
  expect(bridge.calls.map((call) => call.channel)).toEqual(["artifacts.save", "artifacts.save"]);
});

test("a failed action shows the host's wording or mapped copy", async () => {
  const { bridge, store } = actions();
  const id = `${RUN}/plan.pdf`;
  bridge.fail("artifacts.reveal", AppErrorCode.NOT_FOUND, "Файл не найден");
  expect(await store.reveal(id)).toBe(false);
  expect(store.errors[id]).toBe("Файл не найден");
  bridge.fail("artifacts.save", AppErrorCode.PERMISSION_DENIED, "denied");
  expect(await store.save(id)).toBe(false);
  expect(store.errors[id]).toBe(ARTIFACT_ERROR_COPY[AppErrorCode.PERMISSION_DENIED]);
  bridge.handle("artifacts.reveal", () => undefined);
  expect(await store.reveal(id)).toBe(true);
  expect(store.errors[id]).toBeUndefined();
});

const dashboard = {
  id: "dashboards/Продажи 2026.html",
  name: "Продажи 2026.html",
  format: "html",
  sizeBytes: 4096,
};

test("files in agentic tool steps become artifacts and dashboards are named as such", () => {
  expect(
    stepArtifacts([
      { artifacts: [dashboard] },
      { text: "no files" },
      null,
      { artifacts: [{ ...dashboard, id: "../escape.html" }] },
    ]),
  ).toEqual([dashboard]);
  expect(artifactSubtitle(dashboard)).toBe(`${DASHBOARD_KIND} · HTML · ${formatBytes(4096)}`);
  expect(opensAsPage("HTML")).toBe(true);
  expect(opensAsPage("svg")).toBe(true);
  expect(opensAsPage("docx")).toBe(false);
});

test("a page opens in a listed browser or the built-in one, never offering the default twice", async () => {
  const { bridge, store } = actions();
  bridge
    .handle("artifacts.application", () => ({ name: "Google Chrome" }))
    .handle("artifacts.browsers", () => ({
      browsers: [
        { id: "Google Chrome", name: "Google Chrome" },
        { id: "Microsoft Edge", name: "Microsoft Edge" },
      ],
    }))
    .handle("artifacts.openInBrowser", () => undefined)
    .handle("artifacts.openInApp", () => undefined);
  await store.loadApplication(dashboard.id, "html");
  await store.loadBrowsers();
  await store.loadBrowsers();
  expect(store.browsersFor("html").map((browser) => browser.id)).toEqual(["Microsoft Edge"]);
  expect(await store.openInBrowser(dashboard.id, "Microsoft Edge")).toBe(true);
  expect(await store.openInApp(dashboard.id)).toBe(true);
  expect(bridge.calls.map((call) => [call.channel, call.payload])).toEqual([
    ["artifacts.application", { id: dashboard.id }],
    ["artifacts.browsers", undefined],
    ["artifacts.openInBrowser", { id: dashboard.id, browser: "Microsoft Edge" }],
    ["artifacts.openInApp", { id: dashboard.id }],
  ]);
});
