import type { Json } from "@zvs/shared";

type JsonObject = { readonly [key: string]: Json };

export interface SchemaField {
  readonly path: string;
  readonly name: string;
  readonly depth: number;
  readonly type: string;
  readonly required: boolean;
  readonly defaultValue: string | null;
  readonly description: string | null;
}

export interface SchemaView {
  readonly kind: "none" | "fields" | "raw";
  readonly fields: readonly SchemaField[];
  readonly raw: string;
}

const MAX_DEPTH = 3;

function isObject(value: Json | undefined): value is JsonObject {
  return (
    value !== null && value !== undefined && typeof value === "object" && !Array.isArray(value)
  );
}

function propertiesOf(schema: JsonObject): JsonObject | null {
  const properties = schema.properties;
  return isObject(properties) ? properties : null;
}

function requiredOf(schema: JsonObject): ReadonlySet<string> {
  const required = schema.required;
  if (!Array.isArray(required)) return new Set();
  return new Set(required.filter((entry): entry is string => typeof entry === "string"));
}

function literal(value: Json): string {
  return JSON.stringify(value);
}

export function schemaType(schema: Json | undefined): string {
  if (!isObject(schema)) return "any";
  if (schema.const !== undefined) return literal(schema.const);
  if (Array.isArray(schema.enum) && schema.enum.length > 0)
    return schema.enum.map((entry) => literal(entry)).join(" | ");
  const variants = Array.isArray(schema.anyOf)
    ? schema.anyOf
    : Array.isArray(schema.oneOf)
      ? schema.oneOf
      : null;
  if (variants !== null && variants.length > 0)
    return [...new Set(variants.map((variant) => schemaType(variant)))].join(" | ");
  const type = schema.type;
  if (Array.isArray(type))
    return type.filter((entry): entry is string => typeof entry === "string").join(" | ");
  if (type === "array") {
    const items = schema.items;
    const inner = schemaType(items);
    return inner.includes(" ") ? `(${inner})[]` : `${inner}[]`;
  }
  if (typeof type === "string") return type;
  if (propertiesOf(schema) !== null) return "object";
  return "any";
}

function collect(schema: JsonObject, prefix: string, depth: number, into: SchemaField[]): void {
  const properties = propertiesOf(schema);
  if (properties === null) return;
  const required = requiredOf(schema);
  for (const [name, property] of Object.entries(properties)) {
    const path = prefix.length === 0 ? name : `${prefix}.${name}`;
    const node = isObject(property) ? property : {};
    into.push({
      path,
      name,
      depth,
      type: schemaType(node),
      required: required.has(name),
      defaultValue: node.default === undefined ? null : literal(node.default),
      description: typeof node.description === "string" ? node.description : null,
    });
    if (depth + 1 < MAX_DEPTH && propertiesOf(node) !== null) collect(node, path, depth + 1, into);
  }
}

export function describeSchema(schema: Json): SchemaView {
  if (schema === null) return { kind: "none", fields: [], raw: "" };
  const raw = JSON.stringify(schema, null, 2);
  if (!isObject(schema) || propertiesOf(schema) === null) {
    const empty = isObject(schema) && schema.type === "object";
    return { kind: empty ? "none" : "raw", fields: [], raw };
  }
  const fields: SchemaField[] = [];
  collect(schema, "", 0, fields);
  return { kind: fields.length === 0 ? "none" : "fields", fields, raw };
}

function placeholder(schema: JsonObject): Json {
  if (schema.default !== undefined) return schema.default;
  if (schema.const !== undefined) return schema.const;
  if (Array.isArray(schema.enum) && schema.enum.length > 0) return schema.enum[0] ?? null;
  const type = Array.isArray(schema.type) ? schema.type[0] : schema.type;
  switch (type) {
    case "string":
      return "";
    case "number":
    case "integer":
      return 0;
    case "boolean":
      return false;
    case "array":
      return [];
    case "object":
      return seed(schema);
    default:
      return propertiesOf(schema) === null ? null : seed(schema);
  }
}

function seed(schema: JsonObject): Json {
  const properties = propertiesOf(schema);
  if (properties === null) return {};
  const required = requiredOf(schema);
  const value: Record<string, Json> = {};
  for (const [name, property] of Object.entries(properties)) {
    const node = isObject(property) ? property : {};
    if (node.default !== undefined) value[name] = node.default;
    else if (required.has(name)) value[name] = placeholder(node);
  }
  return value;
}

export function seedInput(schema: Json): Json {
  return isObject(schema) ? seed(schema) : {};
}

export function seedText(schema: Json): string {
  return JSON.stringify(seedInput(schema), null, 2);
}

export type ParsedInput =
  { readonly ok: true; readonly value: Json } | { readonly ok: false; readonly error: string };

export function parseInput(text: string): ParsedInput {
  if (text.trim().length === 0) return { ok: true, value: {} };
  try {
    return { ok: true, value: JSON.parse(text) as Json };
  } catch (error: unknown) {
    return {
      ok: false,
      error: error instanceof Error ? `Некорректный JSON: ${error.message}` : "Некорректный JSON",
    };
  }
}
