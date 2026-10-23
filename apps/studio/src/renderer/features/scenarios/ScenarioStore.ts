import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import type { Contract, ScenarioDto, ScenarioId } from "@zvs/shared";
import type { EventRouter } from "../../app/EventRouter";
import ScenarioEditor, { type ScenarioEditorOptions } from "./ScenarioEditor";
import { scenarioErrorCopy } from "./scenarioCopy";

export const NEW_SCENARIO_NAME = "Новый сценарий";

export default class ScenarioStore {
  scenarios: ScenarioDto[] = [];
  editor: ScenarioEditor | null = null;
  loading = false;
  loaded = false;
  creating = false;
  removingId: ScenarioId | null = null;
  error: string | null = null;
  palettePinned = false;
  logHeight: number | undefined = undefined;

  constructor(
    private readonly ipc: IpcClient<Contract>,
    private readonly editorOptions: ScenarioEditorOptions = {},
    private readonly events: EventRouter | null = null,
  ) {
    makeAutoObservable<ScenarioStore, "ipc" | "editorOptions" | "events">(
      this,
      { ipc: false, editorOptions: false, events: false },
      { autoBind: true },
    );
  }

  get isEmpty(): boolean {
    return this.loaded && this.scenarios.length === 0;
  }

  async load(): Promise<void> {
    if (this.loading) return;
    this.loading = true;
    this.error = null;
    try {
      const scenarios = await this.ipc.call("scenarios.list", undefined);
      runInAction(() => {
        this.scenarios = [...scenarios];
        this.loaded = true;
        this.loading = false;
      });
    } catch (error: unknown) {
      runInAction(() => {
        this.error = scenarioErrorCopy(error);
        this.loaded = true;
        this.loading = false;
      });
    }
  }

  async create(name: string = NEW_SCENARIO_NAME): Promise<void> {
    if (this.creating) return;
    this.creating = true;
    this.error = null;
    try {
      const detail = await this.ipc.call("scenarios.create", {
        name: name.trim() || NEW_SCENARIO_NAME,
      });
      runInAction(() => {
        this.creating = false;
      });
      await this.load();
      await this.open(detail.id);
    } catch (error: unknown) {
      runInAction(() => {
        this.error = scenarioErrorCopy(error);
        this.creating = false;
      });
    }
  }

  async open(id: ScenarioId): Promise<void> {
    this.close();
    const editor = new ScenarioEditor(this.ipc, id, {
      ...this.editorOptions,
      ...(this.events === null ? {} : { events: this.events }),
      palettePinned: this.palettePinned,
      ...(this.logHeight === undefined ? {} : { logHeight: this.logHeight }),
    });
    this.editor = editor;
    await editor.load();
  }

  close(): void {
    const editor = this.editor;
    if (editor === null) return;
    this.palettePinned = editor.palettePinned;
    this.logHeight = editor.execution.stripHeight;
    editor.dispose();
    this.editor = null;
    void this.load();
  }

  async remove(id: ScenarioId): Promise<void> {
    if (this.removingId !== null) return;
    this.removingId = id;
    this.error = null;
    try {
      await this.ipc.call("scenarios.remove", { id });
      runInAction(() => {
        this.scenarios = this.scenarios.filter((scenario) => scenario.id !== id);
        this.removingId = null;
      });
    } catch (error: unknown) {
      runInAction(() => {
        this.error = scenarioErrorCopy(error);
        this.removingId = null;
      });
    }
  }

  async rename(name: string): Promise<void> {
    const editor = this.editor;
    const trimmed = name.trim();
    if (editor?.scenario == null || trimmed.length === 0 || trimmed === editor.scenario.name) {
      return;
    }
    try {
      const detail = await this.ipc.call("scenarios.update", { id: editor.id, name: trimmed });
      runInAction(() => {
        if (this.editor === editor && editor.scenario !== null) {
          editor.scenario = { ...editor.scenario, name: detail.name, updatedAt: detail.updatedAt };
        }
      });
    } catch (error: unknown) {
      runInAction(() => {
        editor.error = scenarioErrorCopy(error);
      });
    }
  }

  dismissError(): void {
    this.error = null;
  }

  dispose(): void {
    this.editor?.dispose();
    this.editor = null;
  }
}
