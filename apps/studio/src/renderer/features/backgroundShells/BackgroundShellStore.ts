import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import {
  BackgroundShellStatusDto,
  SHELL_BUFFER_MAX,
  type BackgroundShellDto,
  type BackgroundShellId,
  type Contract,
  type ConversationId,
} from "@zvs/shared";
import type { EventRouter, RoutedEvent } from "../../app/EventRouter";
import { trimBuffer, type TerminalScreen } from "../terminal/TerminalStore";
import { OUTPUT_FAILED, STOP_FAILED, actionErrorCopy } from "./backgroundShellsCopy";

interface Feed {
  buffer: string;
  nextSeq: number;
  stop: () => void;
  screen: TerminalScreen | null;
}

export default class BackgroundShellStore {
  conversationId: ConversationId | null = null;
  shells: BackgroundShellDto[] = [];
  opened: BackgroundShellId[] = [];
  viewing: BackgroundShellId | null = null;
  stopping: BackgroundShellId[] = [];
  error: string | null = null;
  private feeds = new Map<BackgroundShellId, Feed>();
  private generation = 0;
  private stopObserving: (() => void) | null = null;

  constructor(
    private readonly ipc: IpcClient<Contract>,
    private readonly events: EventRouter,
    private readonly clock: () => number = Date.now,
  ) {
    makeAutoObservable<
      BackgroundShellStore,
      "ipc" | "events" | "clock" | "feeds" | "generation" | "stopObserving"
    >(
      this,
      {
        ipc: false,
        events: false,
        clock: false,
        feeds: false,
        generation: false,
        stopObserving: false,
      },
      { autoBind: true },
    );
  }

  get running(): BackgroundShellDto[] {
    return this.shells.filter((shell) => shell.status === "running");
  }

  get tabs(): BackgroundShellDto[] {
    return this.opened
      .map((id) => this.find(id))
      .filter((shell): shell is BackgroundShellDto => shell !== undefined);
  }

  get viewed(): BackgroundShellDto | null {
    return this.viewing === null ? null : (this.find(this.viewing) ?? null);
  }

  find(id: BackgroundShellId): BackgroundShellDto | undefined {
    return this.shells.find((shell) => shell.id === id);
  }

  isStopping(id: BackgroundShellId): boolean {
    return this.stopping.includes(id);
  }

  elapsed(shell: BackgroundShellDto): number {
    return (shell.endedAt ?? this.clock()) - shell.startedAt;
  }

  async show(conversationId: ConversationId | null) {
    this.stopObserving ??= this.events.observe(this.observe);
    if (conversationId === this.conversationId) return;
    this.detachAll();
    this.conversationId = conversationId;
    this.shells = [];
    this.opened = [];
    this.viewing = null;
    this.error = null;
    if (conversationId === null) return;
    await this.reload();
  }

  async reload() {
    const conversationId = this.conversationId;
    if (conversationId === null) return;
    const generation = this.generation;
    try {
      const shells = await this.ipc.call("shells.list", { ownerId: conversationId });
      if (generation !== this.generation) return;
      runInAction(() => (this.shells = shells));
    } catch {
      if (generation === this.generation) runInAction(() => (this.shells = []));
    }
  }

  async view(id: BackgroundShellId) {
    if (this.find(id) === undefined) await this.reload();
    if (this.find(id) === undefined) return;
    if (!this.opened.includes(id)) this.opened = [...this.opened, id];
    this.viewing = id;
    if (this.feeds.has(id)) return;
    await this.attach(id, this.generation);
  }

  leave() {
    this.viewing = null;
  }

  closeTab(id: BackgroundShellId) {
    const index = this.opened.indexOf(id);
    if (index === -1) return;
    this.opened = this.opened.filter((candidate) => candidate !== id);
    this.detach(id);
    if (this.viewing === id) this.viewing = this.opened[index] ?? this.opened[index - 1] ?? null;
  }

  async stop(id: BackgroundShellId) {
    if (this.isStopping(id)) return;
    this.stopping = [...this.stopping, id];
    this.error = null;
    try {
      const shell = await this.ipc.call("shells.kill", { id });
      runInAction(() => this.replace(shell));
    } catch (error: unknown) {
      runInAction(() => (this.error = actionErrorCopy(error, STOP_FAILED)));
    } finally {
      runInAction(() => (this.stopping = this.stopping.filter((candidate) => candidate !== id)));
    }
  }

  connect(id: BackgroundShellId, screen: TerminalScreen): () => void {
    const feed = this.feeds.get(id);
    if (feed === undefined) return () => undefined;
    feed.screen = screen;
    if (feed.buffer !== "") screen.write(feed.buffer);
    return () => {
      if (feed.screen === screen) feed.screen = null;
    };
  }

  write(): void {
    return undefined;
  }

  resize(): void {
    return undefined;
  }

  dismissError() {
    this.error = null;
  }

  dispose() {
    this.detachAll();
    this.stopObserving?.();
    this.stopObserving = null;
    this.conversationId = null;
    this.shells = [];
    this.opened = [];
    this.viewing = null;
  }

  private async attach(id: BackgroundShellId, generation: number) {
    try {
      const output = await this.ipc.call("shells.output", { id });
      if (generation !== this.generation || this.feeds.has(id)) return;
      const feed: Feed = {
        buffer: trimBuffer(output.text, SHELL_BUFFER_MAX),
        nextSeq: output.eventSeq,
        stop: () => undefined,
        screen: null,
      };
      this.feeds.set(id, feed);
      feed.stop = this.events.subscribe(output.shell.streamId, (event) =>
        this.receive(id, feed, event),
      );
      runInAction(() => this.replace(output.shell));
    } catch (error: unknown) {
      if (generation === this.generation) {
        runInAction(() => (this.error = actionErrorCopy(error, OUTPUT_FAILED)));
      }
    }
  }

  private receive(id: BackgroundShellId, feed: Feed, event: RoutedEvent) {
    if (event.seq < feed.nextSeq) return;
    feed.nextSeq = event.seq + 1;
    if (event.type === "token") {
      feed.buffer = trimBuffer(feed.buffer + event.delta, SHELL_BUFFER_MAX);
      feed.screen?.write(event.delta);
      return;
    }
    if (event.type === "end") this.detach(id);
  }

  private observe(event: RoutedEvent) {
    if (event.type !== "step") return;
    const parsed = BackgroundShellStatusDto.safeParse(event.step);
    if (!parsed.success || parsed.data.ownerId !== this.conversationId) return;
    const shell = this.find(parsed.data.id);
    if (shell === undefined) {
      void this.reload();
      return;
    }
    const ended = parsed.data.status !== "running";
    this.replace({
      ...shell,
      status: parsed.data.status,
      exitCode: parsed.data.exitCode,
      endedAt: ended ? (shell.endedAt ?? event.ts) : null,
    });
  }

  private replace(shell: BackgroundShellDto) {
    if (this.find(shell.id) === undefined) {
      if (shell.ownerId === this.conversationId) this.shells = [...this.shells, shell];
      return;
    }
    this.shells = this.shells.map((candidate) => (candidate.id === shell.id ? shell : candidate));
  }

  private detach(id: BackgroundShellId) {
    const feed = this.feeds.get(id);
    feed?.stop();
    this.feeds.delete(id);
  }

  private detachAll() {
    this.generation += 1;
    for (const feed of this.feeds.values()) feed.stop();
    this.feeds.clear();
  }
}
