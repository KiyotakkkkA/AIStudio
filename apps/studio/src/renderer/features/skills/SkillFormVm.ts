import { makeAutoObservable } from "mobx";
import {
  isSkillKey,
  isSkillPath,
  SKILL_DESCRIPTION_MAX,
  SKILL_FILE_MAX_BYTES,
  SKILL_FILES_MAX,
  SKILL_INSTRUCTIONS_MAX,
  SKILL_KEY_MAX,
  SKILL_NAME_MAX,
  SKILL_TOTAL_MAX_BYTES,
  type CreateSkillInput,
  type SkillDetailDto,
  type SkillFileDto,
  type SkillFileInput,
  type SkillId,
  type SkillIssueDto,
  type UpdateSkillInput,
} from "@zvs/shared";
import { issueCopy, issueField, slugify } from "./skillPresentation";

export interface PendingFile {
  readonly path: string;
  readonly size: number;
  readonly content: string;
}

export interface FileRow {
  readonly path: string;
  readonly size: number;
  readonly added: boolean;
}

export const KEY_HINT = "Строчная латиница, цифры и дефисы. Имя папки навыка и ключ для вызова.";

export class SkillFormVm {
  readonly skillId: SkillId | null;
  readonly stored: SkillDetailDto | null;

  key = "";
  name = "";
  description = "";
  instructions = "";
  files: readonly SkillFileDto[] = [];
  added: PendingFile[] = [];
  removed: string[] = [];
  keyTouched = false;

  errors: Record<string, string> = {};
  banner: string | null = null;

  private readonly initial: string;

  constructor(
    options: {
      readonly skill?: SkillDetailDto | null;
      readonly copyOf?: SkillDetailDto | null;
    } = {},
  ) {
    const skill = options.skill ?? null;
    this.stored = skill;
    this.skillId = skill?.id ?? null;
    const source = skill ?? options.copyOf ?? null;
    if (source !== null) {
      this.key = source.key;
      this.name = source.name;
      this.description = source.description;
      this.instructions = source.instructions;
      this.keyTouched = true;
    }
    if (skill !== null) this.files = skill.files;
    this.initial = this.snapshot();
    makeAutoObservable<SkillFormVm, "initial">(this, { initial: false }, { autoBind: true });
  }

  get isNew(): boolean {
    return this.skillId === null;
  }

  get dirty(): boolean {
    return this.isNew || this.snapshot() !== this.initial;
  }

  get lineCount(): number {
    return this.instructions.length === 0 ? 1 : this.instructions.split("\n").length;
  }

  get fileRows(): FileRow[] {
    const kept = this.files
      .filter((file) => !this.removed.includes(file.path))
      .filter((file) => !this.added.some((entry) => entry.path === file.path))
      .map((file) => ({ path: file.path, size: file.size, added: false }));
    const fresh = this.added.map((file) => ({ path: file.path, size: file.size, added: true }));
    return [...kept, ...fresh].sort((left, right) => left.path.localeCompare(right.path));
  }

  get totalSize(): number {
    return this.fileRows.reduce((total, row) => total + row.size, 0);
  }

  setName(name: string): void {
    this.name = name;
    this.clearError("name");
    if (this.isNew && !this.keyTouched) {
      this.key = slugify(name);
      this.clearError("key");
    }
  }

  setKey(key: string): void {
    this.key = key;
    this.keyTouched = true;
    this.clearError("key");
  }

  setDescription(description: string): void {
    this.description = description;
    this.clearError("description");
  }

  setInstructions(instructions: string): void {
    this.instructions = instructions;
    this.clearError("instructions");
  }

  addFile(file: PendingFile): void {
    this.added = [...this.added.filter((entry) => entry.path !== file.path), file];
    this.removed = this.removed.filter((path) => path !== file.path);
    this.clearPrefix("files");
  }

  removeFile(path: string): void {
    if (this.added.some((entry) => entry.path === path)) {
      this.added = this.added.filter((entry) => entry.path !== path);
    } else if (!this.removed.includes(path)) {
      this.removed = [...this.removed, path];
    }
    this.clearPrefix("files");
    this.clearPrefix("removeFiles");
  }

  errorOf(field: string): string | undefined {
    return this.errors[field];
  }

  fileErrorOf(path: string): string | undefined {
    const index = this.added.findIndex((entry) => entry.path === path);
    return index === -1 ? undefined : this.errors[`files.${String(index)}`];
  }

  setBanner(banner: string | null): void {
    this.banner = banner;
  }

  setErrors(errors: Record<string, string>): void {
    this.errors = { ...errors };
  }

  applyIssues(issues: readonly SkillIssueDto[]): void {
    const errors: Record<string, string> = {};
    for (const issue of issues) {
      const key = issue.field.startsWith("files.") ? issue.field : issueField(issue);
      errors[key] ??= issueCopy(issue.rule);
    }
    this.errors = errors;
  }

  validate(): boolean {
    const errors: Record<string, string> = {};
    const key = this.key.trim();
    if (key.length === 0) errors.key = issueCopy("required");
    else if (key.length > SKILL_KEY_MAX) errors.key = issueCopy("tooLong");
    else if (!isSkillKey(key)) errors.key = issueCopy("pattern");

    const name = this.name.trim();
    if (name.length === 0) errors.name = issueCopy("required");
    else if (name.length > SKILL_NAME_MAX) errors.name = issueCopy("tooLong");

    const description = this.description.trim();
    if (description.length === 0) errors.description = issueCopy("required");
    else if (description.length > SKILL_DESCRIPTION_MAX) errors.description = issueCopy("tooLong");

    const instructions = this.instructions.trim();
    if (instructions.length === 0) errors.instructions = issueCopy("required");
    else if (this.instructions.length > SKILL_INSTRUCTIONS_MAX)
      errors.instructions = issueCopy("tooLong");

    this.added.forEach((file, index) => {
      if (!isSkillPath(file.path)) errors[`files.${String(index)}`] = issueCopy("path");
      else if (file.size > SKILL_FILE_MAX_BYTES)
        errors[`files.${String(index)}`] = issueCopy("fileTooLarge");
    });
    const rows = this.fileRows;
    if (rows.length > SKILL_FILES_MAX) errors.files = issueCopy("tooManyFiles");
    else if (this.totalSize > SKILL_TOTAL_MAX_BYTES) errors.files = issueCopy("skillTooLarge");

    this.errors = errors;
    return Object.keys(errors).length === 0;
  }

  toCreateInput(): CreateSkillInput {
    return {
      key: this.key.trim(),
      name: this.name.trim(),
      description: this.description.trim(),
      instructions: this.instructions,
      files: this.fileInputs(),
    };
  }

  toUpdateInput(): UpdateSkillInput {
    if (this.skillId === null) throw new Error("The form is not bound to a skill");
    const input: UpdateSkillInput = {
      id: this.skillId,
      key: this.key.trim(),
      name: this.name.trim(),
      description: this.description.trim(),
      instructions: this.instructions,
    };
    if (this.added.length > 0) input.files = this.fileInputs();
    const removed = this.removed.filter((path) => !this.added.some((entry) => entry.path === path));
    if (removed.length > 0) input.removeFiles = removed;
    return input;
  }

  private fileInputs(): SkillFileInput[] {
    return this.added.map((file) => ({
      path: file.path,
      content: file.content,
      encoding: "base64",
    }));
  }

  private clearPrefix(prefix: string): void {
    const rest: Record<string, string> = {};
    let changed = false;
    for (const [name, message] of Object.entries(this.errors)) {
      if (name === prefix || name.startsWith(`${prefix}.`)) changed = true;
      else rest[name] = message;
    }
    if (changed) this.errors = rest;
  }

  private clearError(key: string): void {
    if (this.errors[key] === undefined) return;
    const rest: Record<string, string> = {};
    for (const [name, message] of Object.entries(this.errors)) {
      if (name !== key) rest[name] = message;
    }
    this.errors = rest;
  }

  private snapshot(): string {
    return JSON.stringify([
      this.key,
      this.name,
      this.description,
      this.instructions,
      this.added.map((file) => [file.path, file.size]),
      [...this.removed].sort(),
    ]);
  }
}
