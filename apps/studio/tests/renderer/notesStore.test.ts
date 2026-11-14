import { describe, expect, test } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  AppError,
  AppErrorCode,
  contract,
  type Contract,
  type ConversationId,
  type NoteDto,
  type NoteId,
  type NoteSummaryDto,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import NotesStore from "../../src/renderer/features/chat/notes/NotesStore.ts";
import {
  NOTE_REMOVED,
  NOTE_RENAME_INVALID,
  NOTES_LOAD_ERROR,
} from "../../src/renderer/features/chat/notes/notesCopy.ts";

const DIALOG = "0199dd11-2222-7111-8111-000000000001" as ConversationId;
const OTHER = "0199dd11-2222-7111-8111-000000000002" as ConversationId;
const NOTE_ID = "0199dd11-4444-7111-8111-000000000001" as NoteId;
const CONFLICT = "Заметка изменилась после открытия";

function note(name: string, content: string, hash = "a".repeat(64)): NoteDto {
  return {
    name,
    path: `chat:notes/${name}`,
    hash,
    content,
    modifiedAt: 1,
    meta: {
      id: NOTE_ID,
      title: "Остановка процессов",
      createdAt: "2026-11-14T12:00:00Z",
      updatedAt: "2026-11-14T12:00:00Z",
      context: "Контекст",
      readWhen: "Перед изменением",
      updateWhen: "При изменении",
      deleteWhen: "После замены",
    },
    body: "Текст",
    error: null,
  };
}

function summary(name: string): NoteSummaryDto {
  return {
    name,
    path: `chat:notes/${name}`,
    size: 10,
    modifiedAt: 1,
    id: NOTE_ID,
    title: "Остановка процессов",
    updatedAt: "2026-11-14T12:00:00Z",
    error: null,
  };
}

function setup() {
  const bridge = createFakeBridge<Contract>();
  let stored: NoteDto | null = note("shutdown.md", "original");
  bridge
    .handle("notes.list", () => (stored === null ? [] : [summary(stored.name)]))
    .handle("notes.read", (input) => {
      const found = stored !== null && (input.name === stored.name || input.id === stored.meta?.id);
      if (!found || stored === null)
        throw new AppError(AppErrorCode.NOT_FOUND, "Заметка не найдена");
      return stored;
    })
    .handle("notes.save", (input) => {
      if (stored === null) throw new AppError(AppErrorCode.NOT_FOUND, "Заметка не найдена");
      if (input.expectedHash !== stored.hash) throw new AppError(AppErrorCode.CONFLICT, CONFLICT);
      stored = note(stored.name, input.content, "b".repeat(64));
      return stored;
    })
    .handle("notes.rename", (input) => {
      if (stored === null) throw new AppError(AppErrorCode.NOT_FOUND, "Заметка не найдена");
      stored = { ...stored, name: input.to, path: `chat:notes/${input.to}` };
      return stored;
    })
    .handle("notes.remove", () => {
      stored = null;
    });
  const store = new NotesStore(createIpcClient(contract, bridge));
  return {
    bridge,
    store,
    replace: (next: NoteDto | null) => {
      stored = next;
    },
  };
}

describe("NotesStore", () => {
  test("shows a dialog's notes and switches cleanly to another dialog", async () => {
    const { store, bridge } = setup();
    await store.show(DIALOG);
    expect(store.notes.map((item) => item.name)).toEqual(["shutdown.md"]);
    await store.open("shutdown.md");
    expect(store.note?.content).toBe("original");
    await store.show(OTHER);
    expect(store.note).toBeNull();
    expect(store.conversationId).toBe(OTHER);
    expect(bridge.calls.filter((call) => call.channel === "notes.list")).toHaveLength(2);
  });

  test("a list failure keeps the panel usable", async () => {
    const { store, bridge } = setup();
    bridge.fail("notes.list", AppErrorCode.UNKNOWN, "x");
    await store.show(DIALOG);
    expect(store.error).toBe(NOTES_LOAD_ERROR);
    expect(store.loaded).toBe(false);
  });

  test("an edit saves with the loaded hash and leaves edit mode", async () => {
    const { store, bridge } = setup();
    await store.show(DIALOG);
    await store.open("shutdown.md");
    store.edit();
    store.setDraft("changed");
    expect(store.dirty).toBe(true);
    expect(await store.save()).toBe(true);
    expect(store.mode).toBe("view");
    expect(store.note?.content).toBe("changed");
    const save = bridge.calls.find((call) => call.channel === "notes.save");
    expect(save?.payload).toMatchObject({ expectedHash: "a".repeat(64), content: "changed" });
  });

  test("a save over a changed file reports the conflict and keeps the draft", async () => {
    const { store, replace } = setup();
    await store.show(DIALOG);
    await store.open("shutdown.md");
    replace(note("shutdown.md", "changed elsewhere", "c".repeat(64)));
    store.edit();
    store.setDraft("mine");
    expect(await store.save()).toBe(false);
    expect(store.conflict).toBe(true);
    expect(store.actionError).toBe(CONFLICT);
    expect(store.draft).toBe("mine");
    await store.reload();
    expect(store.note?.content).toBe("changed elsewhere");
    expect(store.conflict).toBe(false);
  });

  test("a source of a deleted note opens as removed", async () => {
    const { store, replace } = setup();
    replace(null);
    await store.openById(DIALOG, NOTE_ID);
    expect(store.removed).toBe(true);
    expect(store.viewOpen).toBe(true);
    expect(store.note).toBeNull();
  });

  test("a source opens the current version of the note by id", async () => {
    const { store, replace } = setup();
    replace(note("renamed.md", "current"));
    await store.openById(DIALOG, NOTE_ID);
    expect(store.openName).toBe("renamed.md");
    expect(store.note?.content).toBe("current");
  });

  test("rename checks the name and follows the note", async () => {
    const { store } = setup();
    await store.show(DIALOG);
    await store.open("shutdown.md");
    store.startRename();
    store.setRenameDraft("bad/name.md");
    expect(store.renameHint).toBe(NOTE_RENAME_INVALID);
    expect(await store.rename()).toBe(false);
    store.setRenameDraft("process.md");
    expect(await store.rename()).toBe(true);
    expect(store.openName).toBe("process.md");
  });

  test("delete waits for confirmation and closes the open note", async () => {
    const { store, bridge } = setup();
    await store.show(DIALOG);
    await store.open("shutdown.md");
    store.requestRemove("shutdown.md");
    expect(bridge.calls.some((call) => call.channel === "notes.remove")).toBe(false);
    expect(await store.confirmRemove()).toBe(true);
    expect(store.viewOpen).toBe(false);
    expect(store.notes).toEqual([]);
  });

  test("an action on a note deleted meanwhile shows it as removed", async () => {
    const { store, replace } = setup();
    await store.show(DIALOG);
    await store.open("shutdown.md");
    replace(null);
    store.edit();
    store.setDraft("late");
    expect(await store.save()).toBe(false);
    expect(store.removed).toBe(true);
    expect(store.actionError).toBe(NOTE_REMOVED);
  });
});
