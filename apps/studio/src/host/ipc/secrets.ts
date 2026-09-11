import {
  AppError,
  AppErrorCode,
  buildFieldsSchema,
  findSecretTypeSchema,
  isSecretTypeKey,
  SECRET_TYPE_REGISTRY,
  secretFieldOf,
  type Contract,
  type SecretFields,
  type SecretTypeSchema,
} from "@zvs/shared";
import type { IpcHandlers } from "@zvs/ipc";
import { toSecretDto, toSecretSummaryDto, type SecretService } from "../services/SecretService.ts";

export type SecretHandlers = Pick<
  IpcHandlers<Contract>,
  | "secrets.types"
  | "secrets.list"
  | "secrets.get"
  | "secrets.create"
  | "secrets.update"
  | "secrets.remove"
>;

export function createSecretHandlers(secrets: SecretService): SecretHandlers {
  return {
    "secrets.types": () => SECRET_TYPE_REGISTRY.map((schema) => ({ ...schema })),

    "secrets.list": (filter) =>
      secrets
        .list(filter)
        .filter((secret) => isSecretTypeKey(secret.type))
        .map(toSecretSummaryDto),

    "secrets.get": ({ id }) => toSecretDto(requirePublicSecret(secrets, id)),

    "secrets.create": (input) => {
      const schema = requireSchema(input.type);
      requireValuePresence(schema, input.value);
      return toSecretDto(secrets.create({ ...input, fields: parseFields(schema, input.fields) }));
    },

    "secrets.update": ({ id, ...patch }) => {
      const current = requirePublicSecret(secrets, id);
      const schema = requireSchema(current.type);
      return toSecretDto(
        secrets.update(id, {
          ...patch,
          ...(patch.fields === undefined ? {} : { fields: parseFields(schema, patch.fields) }),
        }),
      );
    },

    "secrets.remove": ({ id }) => {
      requirePublicSecret(secrets, id);
      secrets.remove(id);
      return { id, removed: true };
    },
  };
}

function requirePublicSecret(secrets: SecretService, id: string) {
  const secret = secrets.get(id);
  // Account credentials are managed by account services, not the public secret editor.
  if (!isSecretTypeKey(secret.type)) {
    throw new AppError(AppErrorCode.NOT_FOUND, "Секрет не найден");
  }
  return secret;
}

function requireSchema(type: string): SecretTypeSchema {
  const schema = findSecretTypeSchema(type);
  if (schema === undefined) {
    throw new AppError(AppErrorCode.VALIDATION_FAILED, "Неизвестный тип секрета", {
      details: { field: "type", type },
    });
  }
  return schema;
}

function requireValuePresence(schema: SecretTypeSchema, value: string | undefined): void {
  const field = secretFieldOf(schema);
  if (field?.required === true && (value === undefined || value.trim().length === 0)) {
    throw new AppError(AppErrorCode.VALIDATION_FAILED, "Значение секрета обязательно", {
      details: { field: "value", type: schema.key },
    });
  }
}

function parseFields(schema: SecretTypeSchema, fields: SecretFields): SecretFields {
  const parsed = buildFieldsSchema(schema).safeParse(fields);
  if (!parsed.success) {
    throw new AppError(AppErrorCode.VALIDATION_FAILED, "Поля не соответствуют схеме типа", {
      details: {
        field: "fields",
        type: schema.key,
        issues: parsed.error.issues.map((issue) => ({
          path: issue.path.map((segment) => String(segment)).join("."),
          code: issue.code,
        })),
      },
    });
  }
  return parsed.data;
}
