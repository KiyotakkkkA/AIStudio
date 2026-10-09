import { makeAutoObservable } from "mobx";
import {
  McpEnvName,
  McpHeaderName,
  type CreateMcpServerInput,
  type McpServerDto,
  type McpServerId,
  type McpStartup,
  type McpTransport,
  type McpValue,
  type SecretId,
  type UpdateMcpServerInput,
} from "@zvs/shared";
import { hasUnclosedQuote, joinArgs, splitArgs } from "./args";
import type { PresetValue, ServerPreset } from "./serverRegistry";

export type ValueTable = "env" | "headers";
export type ValueKind = McpValue["kind"];

export interface ValueRow {
  readonly id: string;
  name: string;
  kind: ValueKind;
  literal: string;
  secretId: SecretId | null;
}

export const TRANSPORT_FIELDS: Record<McpTransport, readonly string[]> = {
  stdio: ["command", "args", "env"],
  http: ["url", "headers"],
};

let rowSequence = 0;

function newRow(name: string, kind: ValueKind, literal = "", secretId: SecretId | null = null) {
  rowSequence += 1;
  return { id: `row-${String(rowSequence)}`, name, kind, literal, secretId };
}

function rowsOf(values: Readonly<Record<string, McpValue>>): ValueRow[] {
  return Object.entries(values).map(([name, value]) =>
    value.kind === "literal"
      ? newRow(name, "literal", value.value)
      : newRow(name, "secret", "", value.secretId),
  );
}

function rowsOfPreset(values: readonly PresetValue[] | undefined): ValueRow[] {
  return (values ?? []).map((value) => newRow(value.name, value.kind, value.value ?? ""));
}

function recordOf(rows: readonly ValueRow[]): Record<string, McpValue> {
  const record: Record<string, McpValue> = {};
  for (const row of rows) {
    const name = row.name.trim();
    if (row.kind === "literal") record[name] = { kind: "literal", value: row.literal };
    else if (row.secretId !== null) record[name] = { kind: "secret", secretId: row.secretId };
  }
  return record;
}

function isHttpUrl(value: string): boolean {
  try {
    const url = new URL(value);
    return url.protocol === "http:" || url.protocol === "https:";
  } catch {
    return false;
  }
}

export class ServerFormVm {
  readonly serverId: McpServerId | null;
  readonly preset: ServerPreset | null;

  name = "";
  transport: McpTransport = "stdio";
  startup: McpStartup = "manual";
  enabled = true;
  command = "";
  argsText = "";
  url = "";
  env: ValueRow[] = [];
  headers: ValueRow[] = [];

  errors: Record<string, string> = {};
  banner: string | null = null;

  private readonly initial: string;

  constructor(
    options: {
      readonly server?: McpServerDto | null;
      readonly preset?: ServerPreset | null;
    } = {},
  ) {
    const server = options.server ?? null;
    const preset = server === null ? (options.preset ?? null) : null;
    this.serverId = server?.id ?? null;
    this.preset = preset;

    if (server !== null) {
      this.name = server.name;
      this.transport = server.transport;
      this.startup = server.startup;
      this.enabled = server.enabled;
      this.command = server.command ?? "";
      this.argsText = joinArgs(server.args);
      this.url = server.url ?? "";
      this.env = rowsOf(server.env);
      this.headers = rowsOf(server.headers);
    } else if (preset !== null) {
      this.name = preset.name;
      this.transport = preset.transport;
      this.startup = preset.startup;
      this.command = preset.command ?? "";
      this.argsText = joinArgs(preset.args ?? []);
      this.url = preset.url ?? "";
      this.env = rowsOfPreset(preset.env);
      this.headers = rowsOfPreset(preset.headers);
    }

    this.initial = this.snapshot();
    makeAutoObservable<ServerFormVm, "initial">(this, { initial: false }, { autoBind: true });
  }

  get isNew(): boolean {
    return this.serverId === null;
  }

  get dirty(): boolean {
    return this.isNew || this.snapshot() !== this.initial;
  }

  get fields(): readonly string[] {
    return TRANSPORT_FIELDS[this.transport];
  }

  get args(): string[] {
    return splitArgs(this.argsText);
  }

  shows(field: string): boolean {
    return this.fields.includes(field);
  }

  rows(table: ValueTable): readonly ValueRow[] {
    return table === "env" ? this.env : this.headers;
  }

  setName(name: string): void {
    this.name = name;
    this.clearError("name");
  }

  setTransport(transport: McpTransport): void {
    if (transport === this.transport) return;
    this.transport = transport;
    for (const key of Object.keys(this.errors)) {
      if (key !== "name") this.clearError(key);
    }
  }

  setStartup(startup: McpStartup): void {
    this.startup = startup;
  }

  setEnabled(enabled: boolean): void {
    this.enabled = enabled;
  }

  setCommand(command: string): void {
    this.command = command;
    this.clearError("command");
  }

  setArgsText(text: string): void {
    this.argsText = text;
    this.clearError("args");
  }

  setUrl(url: string): void {
    this.url = url;
    this.clearError("url");
  }

  addRow(table: ValueTable, kind: ValueKind): void {
    const row = newRow("", kind);
    if (table === "env") this.env = [...this.env, row];
    else this.headers = [...this.headers, row];
  }

  removeRow(table: ValueTable, id: string): void {
    if (table === "env") this.env = this.env.filter((row) => row.id !== id);
    else this.headers = this.headers.filter((row) => row.id !== id);
    this.clearRowErrors(table);
  }

  setRowName(table: ValueTable, id: string, name: string): void {
    this.patchRow(table, id, { name });
    this.clearError(`${table}.${id}.name`);
  }

  setRowKind(table: ValueTable, id: string, kind: ValueKind): void {
    this.patchRow(table, id, { kind });
    this.clearError(`${table}.${id}.value`);
  }

  setRowLiteral(table: ValueTable, id: string, literal: string): void {
    this.patchRow(table, id, { literal });
    this.clearError(`${table}.${id}.value`);
  }

  setRowSecret(table: ValueTable, id: string, secretId: SecretId | null): void {
    this.patchRow(table, id, { secretId });
    this.clearError(`${table}.${id}.value`);
  }

  errorOf(key: string): string | undefined {
    return this.errors[key];
  }

  rowErrorOf(table: ValueTable, id: string): string | undefined {
    return this.errors[`${table}.${id}.name`] ?? this.errors[`${table}.${id}.value`];
  }

  setErrors(errors: Record<string, string>): void {
    this.errors = { ...errors };
  }

  setBanner(banner: string | null): void {
    this.banner = banner;
  }

  validate(): boolean {
    const errors: Record<string, string> = {};
    const name = this.name.trim();
    if (name.length === 0) errors.name = "Укажите имя сервера.";
    else if (name.length > 120) errors.name = "Не длиннее 120 символов.";

    if (this.transport === "stdio") {
      if (this.command.trim().length === 0) errors.command = "Укажите команду запуска.";
      if (hasUnclosedQuote(this.argsText)) errors.args = "Незакрытая кавычка в аргументах.";
      this.validateRows("env", this.env, McpEnvName, errors);
    } else {
      const url = this.url.trim();
      if (url.length === 0) errors.url = "Укажите адрес сервера.";
      else if (!isHttpUrl(url)) errors.url = "Нужен полный адрес http(s)://…";
      this.validateRows("headers", this.headers, McpHeaderName, errors);
    }

    this.errors = errors;
    return Object.keys(errors).length === 0;
  }

  toCreateInput(): CreateMcpServerInput {
    return { ...this.payload(), enabled: this.enabled };
  }

  toUpdateInput(): UpdateMcpServerInput {
    if (this.serverId === null) throw new Error("Форма не привязана к серверу");
    return { id: this.serverId, ...this.payload() };
  }

  private payload() {
    const stdio = this.transport === "stdio";
    return {
      name: this.name.trim(),
      transport: this.transport,
      startup: this.startup,
      command: stdio ? this.command.trim() : null,
      args: stdio ? this.args : [],
      env: stdio ? recordOf(this.env) : {},
      url: stdio ? null : this.url.trim(),
      headers: stdio ? {} : recordOf(this.headers),
    };
  }

  private validateRows(
    table: ValueTable,
    rows: readonly ValueRow[],
    pattern: typeof McpEnvName,
    errors: Record<string, string>,
  ): void {
    const seen = new Set<string>();
    for (const row of rows) {
      const name = row.name.trim();
      const key = table === "headers" ? name.toLowerCase() : name;
      if (name.length === 0) errors[`${table}.${row.id}.name`] = "Укажите имя.";
      else if (!pattern.safeParse(name).success)
        errors[`${table}.${row.id}.name`] =
          table === "env"
            ? "Латиница, цифры и «_», не с цифры."
            : "Недопустимые символы в имени заголовка.";
      else if (seen.has(key)) errors[`${table}.${row.id}.name`] = "Имя повторяется.";
      seen.add(key);
      if (row.kind === "secret" && row.secretId === null)
        errors[`${table}.${row.id}.value`] = "Выберите секрет.";
    }
  }

  private patchRow(table: ValueTable, id: string, patch: Partial<Omit<ValueRow, "id">>): void {
    const apply = (rows: ValueRow[]) =>
      rows.map((row) => (row.id === id ? { ...row, ...patch } : row));
    if (table === "env") this.env = apply(this.env);
    else this.headers = apply(this.headers);
  }

  private clearRowErrors(table: ValueTable): void {
    for (const key of Object.keys(this.errors)) {
      if (key.startsWith(`${table}.`)) this.clearError(key);
    }
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
    const strip = (rows: readonly ValueRow[]) =>
      rows.map((row) => [row.name, row.kind, row.literal, row.secretId]);
    return JSON.stringify([
      this.name,
      this.transport,
      this.startup,
      this.command,
      this.argsText,
      this.url,
      strip(this.env),
      strip(this.headers),
    ]);
  }
}
