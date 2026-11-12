import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import {
  CLI_STEP_DOMAIN,
  type CliConnectionDto,
  type CliConsentDto,
  type Contract,
} from "@zvs/shared";
import type { EventRouter, RoutedEvent } from "../../app/EventRouter";
import { CLI_CONNECTIONS_FAILED, CLI_DISCONNECT_FAILED } from "./cliCopy";
import { CLI_CONSENT_FAILED } from "./cliConsentCopy";

export default class CliAccessStore {
  consents: CliConsentDto[] = [];
  connections: CliConnectionDto[] = [];
  error: string | null = null;
  consentError: string | null = null;
  answering = false;
  private revision = 0;
  private unobserve: (() => void) | null = null;

  constructor(
    private readonly ipc: IpcClient<Contract>,
    private readonly events: EventRouter,
  ) {
    makeAutoObservable<CliAccessStore, "ipc" | "events" | "revision" | "unobserve">(
      this,
      { ipc: false, events: false, revision: false, unobserve: false },
      { autoBind: true },
    );
  }

  get current(): CliConsentDto | null {
    return this.consents[0] ?? null;
  }

  start() {
    if (this.unobserve !== null) return;
    this.unobserve = this.events.observe(this.receive);
    void this.refresh();
  }

  dispose() {
    this.unobserve?.();
    this.unobserve = null;
  }

  private receive(event: RoutedEvent) {
    if (event.type === "step" && event.step.domain === CLI_STEP_DOMAIN) void this.refresh();
  }

  async refresh() {
    const revision = ++this.revision;
    const [consents, connections] = await Promise.allSettled([
      this.ipc.call("cli.consents.list", undefined),
      this.ipc.call("cli.connections.list", undefined),
    ]);
    runInAction(() => {
      if (revision !== this.revision) return;
      if (consents.status === "fulfilled") this.consents = consents.value;
      if (connections.status === "fulfilled") {
        this.connections = connections.value;
        this.error = null;
      } else {
        this.error = CLI_CONNECTIONS_FAILED;
      }
    });
  }

  async answer(confirm: boolean) {
    const consent = this.current;
    if (consent === null || this.answering) return;
    this.answering = true;
    this.consentError = null;
    try {
      await this.ipc.call("cli.consents.answer", { id: consent.id, confirm });
      runInAction(() => {
        this.consents = this.consents.filter((item) => item.id !== consent.id);
      });
    } catch {
      runInAction(() => {
        this.consentError = CLI_CONSENT_FAILED;
      });
      await this.refresh();
    } finally {
      runInAction(() => {
        this.answering = false;
      });
    }
  }

  async disconnect(id: number) {
    this.error = null;
    try {
      await this.ipc.call("cli.connections.disconnect", { id });
    } catch {
      runInAction(() => {
        this.error = CLI_DISCONNECT_FAILED;
      });
    }
    await this.refresh();
  }
}
