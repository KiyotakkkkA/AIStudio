import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import type {
  Contract,
  McpServerDto,
  ProviderDto,
  RunId,
  RunSummaryDto,
  SkillDetailDto,
  SkillDto,
  SkillId,
  SkillMatchDto,
  SkillModelDto,
  SkillTestResultDto,
  ToolDto,
} from "@zvs/shared";
import { sourceGroups } from "../tools/toolFilters";
import { SkillFormVm, type PendingFile } from "./SkillFormVm";
import {
  SKILL_INVALID_BANNER,
  skillErrorCopy,
  skillIssuesOf,
  toolCeiling,
  type ToolCeiling,
} from "./skillPresentation";

export const MANIFEST_PATH = "SKILL.md";

export interface OpenFile {
  readonly path: string;
  readonly size: number;
  readonly text: string | null;
}

export type PendingIntent =
  | { readonly kind: "create" }
  | { readonly kind: "select"; readonly id: SkillId }
  | { readonly kind: "duplicate" };

const MANAGED_TOOLS: readonly string[] = ["skill.load", "skill.read", "skill.script"];

export interface ToolRow {
  readonly key: string;
  readonly tool: ToolDto | null;
  readonly ceiling: ToolCeiling;
}

export interface ToolGroup {
  readonly key: string;
  readonly label: string;
  readonly rows: readonly ToolRow[];
}

export interface ModelOption {
  readonly value: string;
  readonly label: string;
}

const MODEL_SEPARATOR = "::";
const RUNS_LIMIT = 50;

export function modelValue(model: SkillModelDto | null): string {
  return model === null ? "" : `${model.providerId}${MODEL_SEPARATOR}${model.modelId}`;
}

export function modelOf(value: string): SkillModelDto | null {
  const at = value.indexOf(MODEL_SEPARATOR);
  if (at <= 0) return null;
  return {
    providerId: value.slice(0, at) as SkillModelDto["providerId"],
    modelId: value.slice(at + MODEL_SEPARATOR.length),
  };
}

export default class SkillStore {
  skills: SkillDto[] = [];
  selectedId: SkillId | null = null;
  detail: SkillDetailDto | null = null;
  form: SkillFormVm | null = null;
  openPath: string = MANIFEST_PATH;
  openFile: OpenFile | null = null;
  fileLoading = false;
  advancedOpen = false;
  query = "";
  tools: ToolDto[] = [];
  servers: McpServerDto[] = [];
  providers: ProviderDto[] = [];
  runs: RunSummaryDto[] = [];
  runsLoading = false;
  phrase = "";
  testing = false;
  testResult: SkillTestResultDto | null = null;
  runMessage = "";
  runModel = "";
  running = false;
  lastRunId: RunId | null = null;
  pending: PendingIntent | null = null;
  loading = false;
  loaded = false;
  saving = false;
  removing = false;
  duplicating = false;
  importing = false;
  exporting = false;
  switchingIds: SkillId[] = [];
  error: string | null = null;
  notice: string | null = null;

  private revision = 0;

  constructor(private readonly ipc: IpcClient<Contract>) {
    makeAutoObservable<SkillStore, "ipc" | "revision">(
      this,
      { ipc: false, revision: false },
      { autoBind: true },
    );
  }

  get isEmpty(): boolean {
    return this.loaded && this.skills.length === 0;
  }

  get visibleSkills(): SkillDto[] {
    const query = this.query.trim().toLowerCase();
    if (query.length === 0) return this.skills;
    return this.skills.filter(
      (skill) =>
        skill.name.toLowerCase().includes(query) ||
        skill.key.includes(query) ||
        skill.description.toLowerCase().includes(query),
    );
  }

  get selectedSkill(): SkillDto | null {
    return this.skills.find((skill) => skill.id === this.selectedId) ?? null;
  }

  get toolGroups(): ToolGroup[] {
    const form = this.form;
    const visible = this.tools.filter((tool) => !MANAGED_TOOLS.includes(tool.key));
    const groups: ToolGroup[] = sourceGroups(visible, this.servers).map((group) => ({
      key: group.key,
      label: group.label,
      rows: visible
        .filter(
          (tool) =>
            tool.source === group.source &&
            (group.source === "builtin" || tool.sourceRef === group.sourceRef),
        )
        .sort((left, right) => left.key.localeCompare(right.key))
        .map((tool) => ({
          key: tool.key,
          tool,
          ceiling: toolCeiling(tool, group.server),
        })),
    }));
    const known = new Set(this.tools.map((tool) => tool.key));
    const unknown = (form?.allowedTools ?? []).filter((key) => !known.has(key));
    if (unknown.length > 0) {
      groups.push({
        key: "unknown",
        label: "Нет в реестре",
        rows: unknown.map((key) => ({
          key,
          tool: null,
          ceiling: { block: "unavailable", reason: "Инструмент не найден в реестре." },
        })),
      });
    }
    return groups;
  }

  get allowedCount(): number {
    return this.form?.allowedTools.length ?? 0;
  }

  get modelOptions(): ModelOption[] {
    const options: ModelOption[] = [];
    for (const provider of this.providers) {
      for (const model of provider.models) {
        if (!model.available) continue;
        options.push({
          value: modelValue({ providerId: provider.id, modelId: model.externalId }),
          label: `${provider.name} · ${model.displayName || model.externalId}`,
        });
      }
    }
    const current = this.form?.modelOverride ?? null;
    if (current !== null && !options.some((option) => option.value === modelValue(current))) {
      options.unshift({ value: modelValue(current), label: `${current.modelId} · недоступна` });
    }
    return options;
  }

  get ownMatch(): SkillMatchDto | null {
    const result = this.testResult;
    if (result === null) return null;
    const id = this.form?.isNew === false ? this.selectedId : null;
    return result.matches.find((match) => match.id === id) ?? null;
  }

  get rival(): SkillMatchDto | null {
    const own = this.ownMatch;
    const top = this.testResult?.matches[0] ?? null;
    if (top === null || own === null || top === own || !top.triggered) return null;
    return top.score > own.score ? top : null;
  }

  get effectiveRunModel(): SkillModelDto | null {
    return modelOf(this.runModel) ?? this.selectedSkill?.modelOverride ?? null;
  }

  get runBlocker(): string | null {
    const skill = this.selectedSkill;
    if (skill === null || this.form === null || this.form.isNew) return "Сохраните навык.";
    if (this.form.dirty) return "Есть несохранённые изменения.";
    if (skill.status === "invalid") return "Навык содержит ошибки.";
    if (!skill.enabled) return "Навык выключен.";
    if (this.effectiveRunModel === null) return "Выберите модель.";
    if (this.runMessage.trim().length === 0) return "Введите запрос.";
    return null;
  }

  isSwitching(id: SkillId): boolean {
    return this.switchingIds.includes(id);
  }

  async mount(): Promise<void> {
    await this.load();
  }

  dispose(): void {
    this.revision += 1;
  }

  async load(): Promise<void> {
    this.loading = true;
    this.error = null;
    const revision = ++this.revision;
    try {
      const [skills, tools, servers, summaries] = await Promise.all([
        this.ipc.call("skills.list", undefined),
        this.ipc.call("tools.list", { includeUnavailable: true }),
        this.ipc.call("mcp.list", undefined).catch((): McpServerDto[] => []),
        this.ipc.call("providers.list", { capability: "text", enabled: true }).catch(() => []),
      ]);
      const providers = await Promise.all(
        summaries.map((provider) =>
          this.ipc.call("providers.get", { id: provider.id, selectedOnly: true }),
        ),
      ).catch((): ProviderDto[] => []);
      if (revision !== this.revision) return;
      runInAction(() => {
        this.skills = [...skills];
        this.tools = [...tools];
        this.servers = [...servers];
        this.providers = providers;
        this.loaded = true;
        this.loading = false;
      });
      const selected = this.selectedId;
      if (selected !== null && this.skills.some((skill) => skill.id === selected)) {
        if (this.form?.dirty !== true) await this.select(selected);
      } else if (this.form === null && this.skills[0] !== undefined) {
        await this.select(this.skills[0].id);
      }
    } catch (error: unknown) {
      if (revision !== this.revision) return;
      runInAction(() => {
        this.error = skillErrorCopy(error);
        this.loaded = true;
        this.loading = false;
      });
    }
  }

  setQuery(query: string): void {
    this.query = query;
  }

  setAdvancedOpen(open: boolean): void {
    this.advancedOpen = open;
  }

  async open(path: string): Promise<void> {
    this.openPath = path;
    this.openFile = null;
    const id = this.selectedId;
    const form = this.form;
    if (path === MANIFEST_PATH || id === null || form === null || form.isNew) return;
    if (form.added.some((file) => file.path === path)) return;
    this.fileLoading = true;
    try {
      const content = await this.ipc.call("skills.readFile", { id, path });
      runInAction(() => {
        this.fileLoading = false;
        if (this.selectedId === id && this.openPath === path) this.openFile = content;
      });
    } catch (error: unknown) {
      runInAction(() => {
        this.fileLoading = false;
        this.error = skillErrorCopy(error);
      });
    }
  }

  removeOpenFile(): void {
    const path = this.openPath;
    if (path === MANIFEST_PATH) return;
    this.form?.removeFile(path);
    this.openPath = MANIFEST_PATH;
    this.openFile = null;
  }

  setPhrase(phrase: string): void {
    this.phrase = phrase;
  }

  setRunMessage(message: string): void {
    this.runMessage = message;
  }

  setRunModel(value: string): void {
    this.runModel = value;
  }

  setModelOverride(value: string): void {
    this.form?.setModelOverride(modelOf(value));
  }

  dismissError(): void {
    this.error = null;
  }

  dismissNotice(): void {
    this.notice = null;
  }

  async select(id: SkillId): Promise<void> {
    this.openPath = MANIFEST_PATH;
    this.openFile = null;
    this.selectedId = id;
    this.testResult = null;
    this.runs = [];
    this.lastRunId = null;
    this.runModel = "";
    try {
      const detail = await this.ipc.call("skills.get", { id });
      runInAction(() => {
        if (this.selectedId !== id) return;
        this.detail = detail;
        this.mergeSkill(detail);
        this.form = new SkillFormVm({ skill: detail });
      });
      await this.loadRuns();
    } catch (error: unknown) {
      runInAction(() => {
        if (this.selectedId !== id) return;
        this.error = skillErrorCopy(error);
      });
    }
  }

  requestSelect(id: SkillId): void {
    if (id === this.selectedId && this.form !== null && !this.form.isNew) return;
    if (this.form?.dirty === true) {
      this.pending = { kind: "select", id };
      return;
    }
    void this.select(id);
  }

  startCreate(): void {
    this.selectedId = null;
    this.detail = null;
    this.testResult = null;
    this.runs = [];
    this.openPath = MANIFEST_PATH;
    this.openFile = null;
    this.form = new SkillFormVm();
  }

  requestCreate(): void {
    if (this.form?.dirty === true) {
      this.pending = { kind: "create" };
      return;
    }
    this.startCreate();
  }

  requestDuplicate(): void {
    if (this.form?.dirty === true) {
      this.pending = { kind: "duplicate" };
      return;
    }
    void this.duplicate();
  }

  confirmPending(): void {
    const intent = this.pending;
    this.pending = null;
    if (intent === null) return;
    if (intent.kind === "create") this.startCreate();
    else if (intent.kind === "duplicate") void this.duplicate();
    else void this.select(intent.id);
  }

  cancelPending(): void {
    this.pending = null;
  }

  resetForm(): void {
    const detail = this.detail;
    if (detail === null || this.form?.isNew === true) {
      this.form = null;
      if (this.skills[0] !== undefined) void this.select(this.skills[0].id);
      return;
    }
    this.form = new SkillFormVm({ skill: detail });
  }

  addFiles(files: readonly PendingFile[]): void {
    for (const file of files) this.form?.addFile(file);
  }

  async submit(): Promise<boolean> {
    const form = this.form;
    if (form === null || this.saving) return false;
    form.setBanner(null);
    if (!form.validate()) {
      form.setBanner(SKILL_INVALID_BANNER);
      if (form.errorOf("key") !== undefined) this.advancedOpen = true;
      return false;
    }
    this.saving = true;
    this.error = null;
    try {
      const detail = form.isNew
        ? await this.ipc.call("skills.create", form.toCreateInput())
        : await this.ipc.call("skills.update", form.toUpdateInput());
      runInAction(() => {
        this.saving = false;
        this.mergeSkill(detail);
        this.selectedId = detail.id;
        this.detail = detail;
        this.form = new SkillFormVm({ skill: detail });
      });
      return true;
    } catch (error: unknown) {
      const issues = skillIssuesOf(error);
      runInAction(() => {
        this.saving = false;
        if (this.form === null) {
          this.error = skillErrorCopy(error);
          return;
        }
        if (issues.length > 0) {
          this.form.applyIssues(issues);
          if (
            this.form.errorOf("key") !== undefined ||
            issues.some((issue) => issue.field.startsWith("allowedTools"))
          )
            this.advancedOpen = true;
          this.form.setBanner(SKILL_INVALID_BANNER);
        } else {
          this.form.setBanner(skillErrorCopy(error));
        }
      });
      return false;
    }
  }

  async removeSelected(): Promise<boolean> {
    const id = this.selectedId;
    if (id === null || this.removing) return false;
    this.removing = true;
    this.error = null;
    try {
      await this.ipc.call("skills.remove", { id });
      runInAction(() => {
        this.removing = false;
        this.skills = this.skills.filter((skill) => skill.id !== id);
        this.selectedId = null;
        this.detail = null;
        this.form = null;
        this.runs = [];
        this.testResult = null;
      });
      if (this.skills[0] !== undefined) await this.select(this.skills[0].id);
      return true;
    } catch (error: unknown) {
      runInAction(() => {
        this.removing = false;
        this.error = skillErrorCopy(error);
      });
      return false;
    }
  }

  async setEnabled(id: SkillId, enabled: boolean): Promise<void> {
    if (this.isSwitching(id)) return;
    this.switchingIds = [...this.switchingIds, id];
    this.error = null;
    try {
      const skill = await this.ipc.call("skills.setEnabled", { id, enabled });
      runInAction(() => {
        this.mergeSkill(skill);
        if (this.detail?.id === skill.id) this.detail = { ...this.detail, ...skill };
      });
    } catch (error: unknown) {
      runInAction(() => {
        this.error = skillErrorCopy(error);
      });
    } finally {
      runInAction(() => {
        this.switchingIds = this.switchingIds.filter((entry) => entry !== id);
      });
    }
  }

  async duplicate(): Promise<void> {
    const id = this.selectedId;
    if (id === null || this.duplicating || this.form?.isNew === true) return;
    this.duplicating = true;
    this.error = null;
    try {
      const copy = await this.ipc.call("skills.duplicate", { id });
      runInAction(() => {
        this.duplicating = false;
        this.mergeSkill(copy);
      });
      await this.select(copy.id);
    } catch (error: unknown) {
      runInAction(() => {
        this.duplicating = false;
        this.error = skillErrorCopy(error);
      });
    }
  }

  async importFolder(): Promise<void> {
    if (this.importing) return;
    if (this.form?.dirty === true) {
      this.error = "Сохраните или отмените изменения перед импортом.";
      return;
    }
    this.importing = true;
    this.error = null;
    try {
      const result = await this.ipc.call("skills.import", {});
      runInAction(() => {
        this.importing = false;
      });
      const skill = result.skill;
      if (skill === null) return;
      runInAction(() => {
        this.mergeSkill(skill);
      });
      await this.select(skill.id);
    } catch (error: unknown) {
      runInAction(() => {
        this.importing = false;
        this.error = skillErrorCopy(error);
      });
    }
  }

  async exportSelected(): Promise<void> {
    const id = this.selectedId;
    if (id === null || this.exporting) return;
    this.exporting = true;
    this.error = null;
    this.notice = null;
    try {
      const result = await this.ipc.call("skills.export", { id });
      runInAction(() => {
        this.exporting = false;
        if (result.path !== null) this.notice = `Навык сохранён в папку ${result.path}`;
      });
    } catch (error: unknown) {
      runInAction(() => {
        this.exporting = false;
        this.error = skillErrorCopy(error);
      });
    }
  }

  async testTrigger(): Promise<void> {
    const form = this.form;
    const message = this.phrase.trim();
    if (form === null || message.length === 0 || this.testing) return;
    this.testing = true;
    const description = form.description.trim();
    try {
      const result = await this.ipc.call("skills.test", {
        message,
        id: form.isNew || this.selectedId === null ? undefined : this.selectedId,
        description: description.length > 0 ? description : undefined,
      });
      runInAction(() => {
        this.testing = false;
        this.testResult = result;
      });
    } catch (error: unknown) {
      runInAction(() => {
        this.testing = false;
        this.testResult = null;
        this.error = skillErrorCopy(error);
      });
    }
  }

  async loadRuns(): Promise<void> {
    const id = this.selectedId;
    if (id === null || this.form?.isNew === true) {
      this.runs = [];
      return;
    }
    this.runsLoading = true;
    try {
      const page = await this.ipc.call("runs.list", {
        kinds: ["skill"],
        subjectId: id,
        live: false,
        limit: RUNS_LIMIT,
      });
      runInAction(() => {
        this.runsLoading = false;
        if (this.selectedId === id) this.runs = [...page.items];
      });
    } catch (error: unknown) {
      runInAction(() => {
        this.runsLoading = false;
        this.error = skillErrorCopy(error);
      });
    }
  }

  async run(): Promise<RunId | null> {
    const id = this.selectedId;
    const model = this.effectiveRunModel;
    if (id === null || this.running || this.runBlocker !== null || model === null) return null;
    this.running = true;
    this.error = null;
    try {
      const handle = await this.ipc.call("skills.run", {
        id,
        message: this.runMessage.trim(),
        providerId: model.providerId,
        modelId: model.modelId,
      });
      runInAction(() => {
        this.running = false;
        this.lastRunId = handle.id;
        this.runMessage = "";
      });
      await this.loadRuns();
      return handle.id;
    } catch (error: unknown) {
      runInAction(() => {
        this.running = false;
        this.error = skillErrorCopy(error);
      });
      return null;
    }
  }

  private mergeSkill(skill: SkillDto): void {
    const summary: SkillDto = {
      id: skill.id,
      key: skill.key,
      name: skill.name,
      description: skill.description,
      allowedTools: [...skill.allowedTools],
      modelOverride: skill.modelOverride,
      enabled: skill.enabled,
      source: skill.source,
      status: skill.status,
      issues: [...skill.issues],
      fileCount: skill.fileCount,
      createdAt: skill.createdAt,
      updatedAt: skill.updatedAt,
    };
    const index = this.skills.findIndex((entry) => entry.id === skill.id);
    this.skills =
      index === -1
        ? [...this.skills, summary].sort((left, right) => left.name.localeCompare(right.name))
        : this.skills.map((entry, at) => (at === index ? summary : entry));
  }
}
