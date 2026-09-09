import { and, asc, desc, eq, isNotNull, lte, sql, type SQL } from "drizzle-orm";
import { AppError, AppErrorCode } from "@zvs/shared";
import {
  secret,
  secretUsage,
  type SecretCipher,
  type SecretConsumerKind,
  type SecretInsert,
  type SecretScope,
  type SecretSummary,
  type SecretUsageEntity,
  type SecretUsageInsert,
} from "../schema/index.ts";
import { createId } from "../../platform/ids.ts";
import { Repository } from "./Repository.ts";

export interface SecretFilter {
  scope?: SecretScope;
  type?: string;
  query?: string;
}

export interface SecretDraft extends Omit<SecretInsert, "id"> {
  id?: string;
}

export type SecretPatch = Partial<Omit<SecretInsert, "id" | "createdAt">>;

const SUMMARY_COLUMNS = {
  id: secret.id,
  type: secret.type,
  name: secret.name,
  scope: secret.scope,
  cipherVersion: secret.cipherVersion,
  hint: secret.hint,
  fields: secret.fields,
  tags: secret.tags,
  note: secret.note,
  rotationDays: secret.rotationDays,
  rotatesAt: secret.rotatesAt,
  createdAt: secret.createdAt,
  updatedAt: secret.updatedAt,
};

export class SecretRepository extends Repository {
  listSummaries(filter: SecretFilter = {}): SecretSummary[] {
    const conditions: SQL[] = [];
    if (filter.scope !== undefined) conditions.push(eq(secret.scope, filter.scope));
    if (filter.type !== undefined) conditions.push(eq(secret.type, filter.type));
    const query = filter.query?.trim();
    if (query !== undefined && query.length > 0) {
      conditions.push(sql`${secret.name} LIKE ${`%${escapeLike(query)}%`} ESCAPE '!'`);
    }
    return this.db
      .select(SUMMARY_COLUMNS)
      .from(secret)
      .where(conditions.length === 0 ? undefined : and(...conditions))
      .orderBy(desc(secret.updatedAt), asc(secret.name))
      .all();
  }

  findById(id: string): SecretSummary | undefined {
    return this.db.select(SUMMARY_COLUMNS).from(secret).where(eq(secret.id, id)).get();
  }

  findCipherById(id: string): SecretCipher | undefined {
    return this.db
      .select({ id: secret.id, cipher: secret.cipher, cipherVersion: secret.cipherVersion })
      .from(secret)
      .where(eq(secret.id, id))
      .get();
  }

  listRotatingBefore(ts: number): SecretSummary[] {
    return this.db
      .select(SUMMARY_COLUMNS)
      .from(secret)
      .where(and(isNotNull(secret.rotatesAt), lte(secret.rotatesAt, ts)))
      .orderBy(asc(secret.rotatesAt))
      .all();
  }

  create(draft: SecretDraft): SecretSummary {
    return this.db
      .insert(secret)
      .values({ ...draft, id: draft.id ?? createId() })
      .returning(SUMMARY_COLUMNS)
      .get();
  }

  update(id: string, patch: SecretPatch): SecretSummary | undefined {
    if (Object.keys(patch).length === 0) return this.findById(id);
    return this.db
      .update(secret)
      .set(patch)
      .where(eq(secret.id, id))
      .returning(SUMMARY_COLUMNS)
      .get();
  }

  remove(id: string): void {
    const usage = this.listUsage(id);
    if (usage.length > 0) {
      throw new AppError(
        AppErrorCode.CONFLICT,
        `Секрет используется (${usage.length}) и не может быть удалён`,
        {
          details: {
            secretId: id,
            consumers: usage.map((entry) => ({
              kind: entry.consumerKind,
              id: entry.consumerId,
            })),
          },
        },
      );
    }
    this.db.delete(secret).where(eq(secret.id, id)).run();
  }

  countUsage(id: string): number {
    const row = this.db
      .select({ total: sql<number>`count(*)` })
      .from(secretUsage)
      .where(eq(secretUsage.secretId, id))
      .get();
    return row?.total ?? 0;
  }

  listUsage(id: string): SecretUsageEntity[] {
    return this.db
      .select()
      .from(secretUsage)
      .where(eq(secretUsage.secretId, id))
      .orderBy(asc(secretUsage.consumerKind), asc(secretUsage.consumerId))
      .all();
  }

  addUsage(entry: SecretUsageInsert): SecretUsageEntity {
    return this.db
      .insert(secretUsage)
      .values(entry)
      .onConflictDoUpdate({
        target: [secretUsage.secretId, secretUsage.consumerKind, secretUsage.consumerId],
        set: { createdAt: entry.createdAt },
      })
      .returning()
      .get();
  }

  removeUsage(secretId: string, consumerKind: SecretConsumerKind, consumerId: string): void {
    this.db
      .delete(secretUsage)
      .where(
        and(
          eq(secretUsage.secretId, secretId),
          eq(secretUsage.consumerKind, consumerKind),
          eq(secretUsage.consumerId, consumerId),
        ),
      )
      .run();
  }
}

function escapeLike(value: string): string {
  return value.replace(/[!%_]/g, "!$&");
}
