import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test } from "vitest";
import { ChatToolCallDto, type Json } from "@zvs/shared";
import ChatToolCalls from "../../src/renderer/features/chat/ChatToolCalls";
import {
  AUTHORISATION_TITLE,
  TOOL_PRESENTATION,
  authorisationView,
  isBuiltinTool,
  lineRange,
  toolPresentation,
  toolTitle,
} from "../../src/renderer/features/chat/toolCallCopy.ts";
import { resultLines, toolCallView } from "../../src/renderer/features/chat/toolCallView.ts";
import { toolCallSummary } from "../../src/renderer/features/chat/toolCallSummary.ts";
import { EXEC_MODE_COPY } from "../../src/renderer/features/execPolicy/execPolicyCopy.ts";

const call = (key: string, args: Json, extra: Partial<ChatToolCallDto> = {}) =>
  ChatToolCallDto.parse({
    id: `call_${key}`,
    key,
    name: key.replaceAll(".", "_"),
    round: 0,
    arguments: args,
    status: "succeeded",
    ...extra,
  });

const citation = {
  storeId: "0199bb11-7777-7111-8111-000000000001",
  documentId: "doc-1",
  sourcePath: "C:/docs/policy/vacation.pdf",
  chunkIndex: 2,
  score: 0.82,
  section: "Отпуск",
  page: 4,
};

test("every built-in tool has a Russian title and unknown tools keep their key", () => {
  for (const [key, presentation] of Object.entries(TOOL_PRESENTATION)) {
    expect(presentation.title).not.toBe(key);
    expect(isBuiltinTool(key)).toBe(true);
  }
  expect(toolTitle("fs.read")).toBe(TOOL_PRESENTATION["fs.read"]!.title);
  expect(isBuiltinTool("mcp.docs.search")).toBe(false);
  expect(toolPresentation("mcp.docs.search")).toMatchObject({
    title: "mcp.docs.search",
    category: "other",
  });
});

test("search calls carry the query, the store filter and the cited fragments", () => {
  const view = toolCallView(
    call(
      "knowledge.search",
      { query: "отпуск", store: "Кадры", limit: 4 },
      { meta: { query: "отпуск", citations: [citation] } },
    ),
  );
  expect(view).toMatchObject({ kind: "search", query: "отпуск", store: "Кадры", limit: 4 });
  expect(view?.kind === "search" ? view.citations?.map((item) => item.documentId) : null).toEqual([
    "doc-1",
  ]);

  const vector = toolCallView(
    call("vector.search", { query: "q", storeId: citation.storeId, k: 3 }),
  );
  expect(vector).toMatchObject({ kind: "search", store: null, limit: 3, citations: null });
});

test("a document read names the cited source instead of the raw id", () => {
  expect(
    toolCallView(
      call(
        "knowledge.read",
        { documentId: "doc-1", fromChunk: 0, toChunk: 3 },
        { meta: { citations: [citation] } },
      ),
    ),
  ).toEqual({ kind: "document", document: citation.sourcePath, section: null, from: 0, to: 3 });
  expect(toolCallView(call("knowledge.read", { documentId: "doc-9" }))).toMatchObject({
    document: "doc-9",
  });
});

test("file tools turn their arguments into folders, files and search flags", () => {
  expect(
    toolCallView(call("fs.list", { recursive: true, glob: "**/*.ts", conversationId: "c" })),
  ).toEqual({ kind: "folder", path: null, recursive: true, glob: "**/*.ts", limit: null });
  expect(
    toolCallView(
      call("fs.read", {
        paths: [{ path: "src/a.ts", startLine: 10, endLine: 40 }, { path: "README.md" }, { x: 1 }],
      }),
    ),
  ).toEqual({
    kind: "files",
    files: [
      { path: "src/a.ts", start: 10, end: 40 },
      { path: "README.md", start: null, end: null },
    ],
  });
  expect(
    toolCallView(call("fs.search", { query: "TODO\\b", regex: true, ignoreCase: true })),
  ).toMatchObject({ kind: "grep", regex: true, ignoreCase: true, path: null });
  expect(toolCallView(call("fs.read", { paths: [] }))).toBeNull();
});

test("process, skill and job calls get their own views and unknown calls fall back", () => {
  expect(toolCallView(call("shell.run", { argv: ["npm", "test"], cwd: "app" }))).toEqual({
    kind: "command",
    argv: ["npm", "test"],
    cwd: "app",
  });
  expect(
    toolCallView(call("code.run", { language: "python", source: "print(1)", packages: [] })),
  ).toMatchObject({ kind: "program", language: "Python", source: "print(1)" });
  expect(
    toolCallView(call("skill.script", { key: "docx", path: "scripts/a.py", args: ["-v"] })),
  ).toMatchObject({ kind: "skill", skill: "docx", path: "scripts/a.py", args: ["-v"] });
  expect(toolCallView(call("job.run", { job: "reindex", params: { a: 1 } }))).toEqual({
    kind: "fields",
    fields: [{ label: "job", value: "reindex" }],
  });
  expect(toolCallView(call("knowledge.list", {}))).toBeNull();
  expect(toolCallView(call("mcp.docs.search", { query: "x" }))).toBeNull();
  expect(toolCallView(call("fs.list", "not an object"))).toBeNull();
});

test("line ranges and result sizes read naturally", () => {
  expect(lineRange(null, null)).toBe("целиком");
  expect(lineRange(5, null)).toBe("строки 5–конец");
  expect(lineRange(null, 9)).toBe("строки 1–9");
  expect(resultLines("")).toBe(0);
  expect(resultLines("a\nb\nc")).toBe(3);
});

test("a card shows the Russian title, opens itself for an approval and collapses on click", () => {
  const pending = call(
    "fs.read",
    { paths: [{ path: "src/a.ts" }] },
    { status: "awaiting-approval", approvalId: "0199bb11-5555-7111-8111-000000000009" },
  );
  render(
    <ChatToolCalls
      calls={[pending]}
      approvalFor={(id) => (id === null ? undefined : { id, busy: false })}
    />,
  );
  const header = screen.getByRole("button", { name: new RegExp(toolTitle("fs.read")) });
  expect(header.getAttribute("aria-expanded")).toBe("true");
  expect(screen.getAllByText("src/a.ts")).toHaveLength(2);
  fireEvent.click(header);
  expect(header.getAttribute("aria-expanded")).toBe("false");
});

test("collapsed summaries describe file, skill and search calls instead of raw JSON", () => {
  expect(toolCallSummary(call("fs.list", { recursive: false }))).toBe("рабочая папка");
  expect(toolCallSummary(call("fs.list", { path: "src", glob: "*.ts" }))).toBe("src · *.ts");
  expect(
    toolCallSummary(
      call("fs.read", { paths: [{ path: "a.ts" }, { path: "b.ts" }, { path: "c.ts" }] }),
    ),
  ).toBe("a.ts, b.ts и ещё 1");
  expect(toolCallSummary(call("fs.search", { query: "TODO" }))).toBe("«TODO»");
  expect(toolCallSummary(call("skill.read", { key: "pdf", path: "SKILL.md" }))).toBe(
    "pdf · SKILL.md",
  );
  expect(toolCallSummary(call("vector.search", { query: "q", storeId: "s" }))).toBe("«q»");
});

test("an authorisation names the mode, or keeps the judge source and its reason", () => {
  const policy = authorisationView({ kind: "policy", source: "safety-ask" });
  expect(policy.label).toContain(EXEC_MODE_COPY["safety-ask"].label);
  expect(policy.source).toBeNull();
  expect(policy.reason).toBeNull();
  const judged = authorisationView({
    kind: "judge",
    source: "gpt-oss:120b",
    reason: "Удаляется временный файл.",
  });
  expect(judged.source).toBe("gpt-oss:120b");
  expect(judged.reason).toBe("Удаляется временный файл.");
  expect(authorisationView({ kind: "approved" }).source).toBeNull();
});

test("the expanded call shows why it ran", () => {
  render(
    <ChatToolCalls
      calls={[
        call(
          "fs.apply",
          { changes: [] },
          {
            status: "succeeded",
            authorisation: { kind: "judge", source: "judge-model", reason: "Причина решения." },
          },
        ),
      ]}
    />,
  );
  fireEvent.click(screen.getByRole("button", { expanded: false }));
  expect(screen.getByText(AUTHORISATION_TITLE)).toBeTruthy();
  expect(screen.getByText("Причина решения.")).toBeTruthy();
  expect(screen.getByText("judge-model")).toBeTruthy();
});
