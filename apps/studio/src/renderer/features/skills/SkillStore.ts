import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import type { Contract, SkillDetailDto, SkillDto, SkillId } from "@zvs/shared";
import { SkillFormVm, type PendingFile } from "./SkillFormVm";
import {
  SKILL_INVALID_BANNER,
  skillErrorCopy,
  skillIssuesOf,
  UPDATE_GUARD,
  updatedAllNotice,
  updatedNotice,
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

export default class SkillStore {
  skills: SkillDto[] = [];
  selectedId: SkillId | null = null;
  detail: SkillDetailDto | null = null;
  form: SkillFormVm | null = null;
  openPath: string = MANIFEST_PATH;
  openFile: OpenFile | null = null;
  fileLoading = false;
  query = "";
  pending: PendingIntent | null = null;
  loading = false;
  loaded = false;
  saving = false;
  removing = false;
  duplicating = false;
  importing = false;
  exporting = false;
  updatingAll = false;
  switchingIds: SkillId[] = [];
  updatingIds: SkillId[] = [];
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

  get updatable(): SkillDto[] {
    return this.skills.filter((skill) => skill.update !== null);
  }

  get modifiedUpdates(): number {
    return this.updatable.filter((skill) => skill.update?.modified === true).length;
  }

  get selectedSkill(): SkillDto | null {
    return this.skills.find((skill) => skill.id === this.selectedId) ?? null;
  }

  isSwitching(id: SkillId): boolean {
    return this.switchingIds.includes(id);
  }

  isUpdating(id: SkillId): boolean {
    return this.updatingAll || this.updatingIds.includes(id);
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
      const skills = await this.ipc.call("skills.list", undefined);
      if (revision !== this.revision) return;
      runInAction(() => {
        this.skills = [...skills];
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
    try {
      const detail = await this.ipc.call("skills.get", { id });
      runInAction(() => {
        if (this.selectedId !== id) return;
        this.detail = detail;
        this.mergeSkill(detail);
        this.form = new SkillFormVm({ skill: detail });
      });
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

  async updateSkill(id: SkillId): Promise<void> {
    if (this.isUpdating(id)) return;
    if (this.selectedId === id && this.form?.dirty === true) {
      this.error = UPDATE_GUARD;
      return;
    }
    this.updatingIds = [...this.updatingIds, id];
    this.error = null;
    this.notice = null;
    try {
      const detail = await this.ipc.call("skills.updateBundled", { id });
      runInAction(() => {
        this.mergeSkill(detail);
        this.notice = updatedNotice(detail.name, detail.version);
        if (this.selectedId === detail.id) {
          this.detail = detail;
          this.form = new SkillFormVm({ skill: detail });
          this.openPath = MANIFEST_PATH;
          this.openFile = null;
        }
      });
    } catch (error: unknown) {
      runInAction(() => {
        this.error = skillErrorCopy(error);
      });
    } finally {
      runInAction(() => {
        this.updatingIds = this.updatingIds.filter((entry) => entry !== id);
      });
    }
  }

  async updateAll(): Promise<void> {
    if (this.updatingAll) return;
    if (this.form?.dirty === true && (this.selectedSkill?.update ?? null) !== null) {
      this.error = UPDATE_GUARD;
      return;
    }
    const count = this.updatable.length;
    if (count === 0) return;
    this.updatingAll = true;
    this.error = null;
    this.notice = null;
    try {
      const skills = await this.ipc.call("skills.updateAllBundled", undefined);
      runInAction(() => {
        this.skills = [...skills];
        this.updatingAll = false;
        this.notice = updatedAllNotice(count);
      });
      const selected = this.selectedId;
      if (selected !== null && this.form?.dirty !== true) await this.select(selected);
    } catch (error: unknown) {
      const message = skillErrorCopy(error);
      await this.load();
      runInAction(() => {
        this.updatingAll = false;
        this.error = message;
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

  private mergeSkill(skill: SkillDto): void {
    const summary: SkillDto = {
      id: skill.id,
      key: skill.key,
      name: skill.name,
      description: skill.description,
      enabled: skill.enabled,
      source: skill.source,
      status: skill.status,
      issues: [...skill.issues],
      fileCount: skill.fileCount,
      version: skill.version,
      author: skill.author,
      update: skill.update,
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
