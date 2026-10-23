import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import {
  TERMINAL_SCROLLBACK_MAX,
  TerminalStatusDto,
  type Contract,
  type ConversationId,
  type TerminalId,
  type TerminalSessionDto,
  type TerminalShell,
} from "@zvs/shared";
import type { EventRouter, RoutedEvent } from "../../app/EventRouter";
import { actionErrorCopy, createErrorCopy } from "./terminalCopy";

export interface TerminalScreen {
  write(text: string): void;
  reset(): void;
}

export const DOCK_HEIGHT_MIN = 140;
export const DOCK_HEIGHT_DEFAULT = 280;
export const DEFAULT_COLS = 120;
export const DEFAULT_ROWS = 30;

interface Feed {
  buffer: string;
  nextSeq: number;
  stop: () => void;
  screen: TerminalScreen | null;
}

export function trimBuffer(buffer: string, max = TERMINAL_SCROLLBACK_MAX): string {
  if (buffer.length <= max) return buffer;
  const cut = buffer.length - max;
  const line = buffer.indexOf("\n", cut);
  return buffer.slice(line === -1 ? cut : line + 1);
}

export default class TerminalStore {
  open = false;
  height = DOCK_HEIGHT_DEFAULT;
  conversationId: ConversationId | null = null;
  sessions: TerminalSessionDto[] = [];
  activeId: TerminalId | null = null;
  shells: TerminalShell[] = [];
  creating = false;
  loading = false;
  error: string | null = null;
  private feeds = new Map<TerminalId, Feed>();
  private sizes = new Map<TerminalId, string>();
  private generation = 0;

  constructor(
    private readonly ipc: IpcClient<Contract>,
    private readonly events: EventRouter,
  ) {
    makeAutoObservable<TerminalStore, "ipc" | "events" | "feeds" | "sizes" | "generation">(
      this,
      { ipc: false, events: false, feeds: false, sizes: false, generation: false },
      { autoBind: true },
    );
  }

  get active(): TerminalSessionDto | null {
    return this.sessions.find((session) => session.id === this.activeId) ?? null;
  }

  setHeight(height: number) {
    this.height = Math.max(DOCK_HEIGHT_MIN, Math.round(height));
  }

  async show(conversationId: ConversationId | null) {
    if (conversationId === this.conversationId) return;
    this.detachAll();
    this.conversationId = conversationId;
    this.sessions = [];
    this.activeId = null;
    this.error = null;
    if (conversationId === null) return;
    const generation = this.generation;
    this.loading = true;
    try {
      const sessions = await this.ipc.call("terminal.list", { id: conversationId });
      for (const session of sessions) {
        if (generation !== this.generation) return;
        await this.attach(session.id, generation);
      }
    } catch (error: unknown) {
      if (generation === this.generation) runInAction(() => (this.error = actionErrorCopy(error)));
    } finally {
      if (generation === this.generation) runInAction(() => (this.loading = false));
    }
    if (generation === this.generation && this.open && this.sessions.length === 0) {
      await this.create(null);
    }
  }

  async toggle() {
    this.open = !this.open;
    if (!this.open) return;
    if (this.shells.length === 0) void this.loadShells();
    if (this.conversationId !== null && this.sessions.length === 0 && !this.loading) {
      await this.create(null);
    }
  }

  hide() {
    this.open = false;
  }

  select(id: TerminalId) {
    if (this.sessions.some((session) => session.id === id)) this.activeId = id;
  }

  async loadShells() {
    try {
      const shells = await this.ipc.call("terminal.shells", undefined);
      runInAction(() => (this.shells = shells));
    } catch {
      runInAction(() => (this.shells = []));
    }
  }

  async create(shell: TerminalShell | null) {
    const conversationId = this.conversationId;
    if (conversationId === null || this.creating) return;
    const generation = this.generation;
    this.creating = true;
    this.error = null;
    try {
      const session = await this.ipc.call("terminal.create", {
        conversationId,
        shell,
        cols: DEFAULT_COLS,
        rows: DEFAULT_ROWS,
      });
      if (generation !== this.generation) return;
      await this.attach(session.id, generation);
      runInAction(() => (this.activeId = session.id));
    } catch (error: unknown) {
      if (generation === this.generation) runInAction(() => (this.error = createErrorCopy(error)));
    } finally {
      if (generation === this.generation) runInAction(() => (this.creating = false));
    }
  }

  connect(id: TerminalId, screen: TerminalScreen): () => void {
    const feed = this.feeds.get(id);
    if (feed === undefined) return () => undefined;
    feed.screen = screen;
    if (feed.buffer !== "") screen.write(feed.buffer);
    return () => {
      if (feed.screen === screen) feed.screen = null;
    };
  }

  write(id: TerminalId, data: string) {
    if (data === "") return;
    const session = this.sessions.find((candidate) => candidate.id === id);
    if (session?.status !== "running") return;
    this.ipc.call("terminal.write", { id, data }).catch((error: unknown) => {
      runInAction(() => (this.error = actionErrorCopy(error)));
    });
  }

  interrupt(id: TerminalId) {
    this.write(id, "\u0003");
  }

  resize(id: TerminalId, cols: number, rows: number) {
    const key = `${String(cols)}x${String(rows)}`;
    if (this.sizes.get(id) === key) return;
    this.sizes.set(id, key);
    void this.ipc.call("terminal.resize", { id, cols, rows }).catch(() => undefined);
  }

  async restart(id: TerminalId) {
    try {
      const session = await this.ipc.call("terminal.restart", { id });
      runInAction(() => this.replace(session));
    } catch (error: unknown) {
      runInAction(() => (this.error = actionErrorCopy(error)));
    }
  }

  async clear(id: TerminalId) {
    const feed = this.feeds.get(id);
    if (feed !== undefined) {
      feed.buffer = "";
      feed.screen?.reset();
    }
    await this.ipc.call("terminal.clear", { id }).catch(() => undefined);
  }

  async rename(id: TerminalId, title: string) {
    const trimmed = title.trim();
    if (trimmed === "") return;
    try {
      const session = await this.ipc.call("terminal.rename", { id, title: trimmed });
      runInAction(() => this.replace(session));
    } catch (error: unknown) {
      runInAction(() => (this.error = actionErrorCopy(error)));
    }
  }

  async close(id: TerminalId) {
    this.forget(id);
    await this.ipc.call("terminal.close", { id }).catch(() => undefined);
  }

  dismissError() {
    this.error = null;
  }

  dispose() {
    this.detachAll();
    this.conversationId = null;
    this.sessions = [];
    this.activeId = null;
  }

  private async attach(id: TerminalId, generation: number) {
    const attached = await this.ipc.call("terminal.attach", { id });
    if (generation !== this.generation || this.feeds.has(id)) return;
    const feed: Feed = {
      buffer: trimBuffer(attached.scrollback),
      nextSeq: attached.nextSeq,
      stop: () => undefined,
      screen: null,
    };
    this.feeds.set(id, feed);
    feed.stop = this.events.subscribe(attached.session.streamId, (event) =>
      this.receive(id, feed, event),
    );
    runInAction(() => {
      this.sessions = [...this.sessions, attached.session];
      this.activeId ??= id;
    });
  }

  private receive(id: TerminalId, feed: Feed, event: RoutedEvent) {
    if (event.seq < feed.nextSeq) return;
    feed.nextSeq = event.seq + 1;
    if (event.type === "token") {
      feed.buffer = trimBuffer(feed.buffer + event.delta);
      feed.screen?.write(event.delta);
      return;
    }
    if (event.type === "end") {
      this.forget(id);
      return;
    }
    if (event.type !== "step") return;
    const parsed = TerminalStatusDto.safeParse(event.step);
    if (!parsed.success || parsed.data.id !== id) return;
    const session = this.sessions.find((candidate) => candidate.id === id);
    if (session === undefined) return;
    if (parsed.data.status === "running") {
      feed.buffer = "";
      feed.screen?.reset();
      this.sizes.delete(id);
    }
    this.replace({ ...session, status: parsed.data.status, exitCode: parsed.data.exitCode });
  }

  private replace(session: TerminalSessionDto) {
    this.sessions = this.sessions.map((candidate) =>
      candidate.id === session.id ? session : candidate,
    );
  }

  private forget(id: TerminalId) {
    const feed = this.feeds.get(id);
    feed?.stop();
    this.feeds.delete(id);
    this.sizes.delete(id);
    const index = this.sessions.findIndex((session) => session.id === id);
    if (index === -1) return;
    this.sessions = this.sessions.filter((session) => session.id !== id);
    if (this.activeId === id) {
      this.activeId = (this.sessions[index] ?? this.sessions[index - 1] ?? null)?.id ?? null;
    }
  }

  private detachAll() {
    this.generation += 1;
    for (const feed of this.feeds.values()) feed.stop();
    this.feeds.clear();
    this.sizes.clear();
    this.creating = false;
    this.loading = false;
  }
}
