import { and, asc, eq } from "drizzle-orm";
import type { AccountFamily, AccountStatus } from "@zvs/shared";
import { account, provider, type AccountEntity, type AccountInsert } from "../schema/index.ts";
import { createId } from "../../platform/ids.ts";
import { Repository } from "./Repository.ts";
import { SecretRepository } from "./SecretRepository.ts";

export type AccountDraft = Omit<AccountInsert, "id"> & { id?: string };
export type AccountPatch = Partial<Omit<AccountInsert, "id" | "adapter" | "linkedAt">>;

export interface AccountRemoval {
  readonly removed: boolean;
  readonly freedSecretId: string | null;
  readonly detachedProviderIds: readonly string[];
}

export class AccountRepository extends Repository {
  create(draft: AccountDraft): AccountEntity {
    return this.db.transaction((tx) => {
      const row = tx
        .insert(account)
        .values({ ...draft, id: draft.id ?? createId() })
        .returning()
        .get();
      if (row.tokenSecretId !== null) {
        new SecretRepository(tx).addUsage({
          secretId: row.tokenSecretId,
          consumerKind: "account",
          consumerId: row.id,
          createdAt: row.linkedAt,
        });
      }
      return row;
    });
  }

  getById(id: string): AccountEntity | undefined {
    return this.db.select().from(account).where(eq(account.id, id)).get();
  }

  findByAdapterAndExternalId(
    adapter: AccountFamily,
    externalId: string,
  ): AccountEntity | undefined {
    return this.db
      .select()
      .from(account)
      .where(and(eq(account.adapter, adapter), eq(account.externalId, externalId)))
      .get();
  }

  listByAdapter(adapter: AccountFamily): AccountEntity[] {
    return this.db
      .select()
      .from(account)
      .where(eq(account.adapter, adapter))
      .orderBy(asc(account.linkedAt), asc(account.id))
      .all();
  }

  update(id: string, patch: AccountPatch): AccountEntity | undefined {
    return this.db.transaction((tx) => {
      const previous = new AccountRepository(tx).getById(id);
      if (previous === undefined || Object.keys(patch).length === 0) return previous;
      const row = tx
        .update(account)
        .set({ ...patch, updatedAt: patch.updatedAt ?? Date.now() })
        .where(eq(account.id, id))
        .returning()
        .get();
      if (row !== undefined && previous.tokenSecretId !== row.tokenSecretId) {
        const secrets = new SecretRepository(tx);
        if (previous.tokenSecretId !== null) {
          secrets.removeUsage(previous.tokenSecretId, "account", id);
        }
        if (row.tokenSecretId !== null) {
          secrets.addUsage({
            secretId: row.tokenSecretId,
            consumerKind: "account",
            consumerId: id,
            createdAt: row.updatedAt,
          });
        }
      }
      return row;
    });
  }

  updateStatus(
    id: string,
    status: AccountStatus,
    detail: string | null = null,
    now = Date.now(),
  ): AccountEntity | undefined {
    return this.update(id, {
      status,
      statusDetail: detail,
      lastCheckedAt: now,
      updatedAt: now,
    });
  }

  remove(id: string, now = Date.now()): AccountRemoval {
    return this.db.transaction((tx) => {
      const row = new AccountRepository(tx).getById(id);
      if (row === undefined) {
        return { removed: false, freedSecretId: null, detachedProviderIds: [] };
      }
      const detached = tx
        .update(provider)
        .set({
          accountId: null,
          status: "needs-relink",
          statusDetail: "Аккаунт отвязан",
          updatedAt: now,
        })
        .where(eq(provider.accountId, id))
        .returning({ id: provider.id })
        .all();
      if (row.tokenSecretId !== null) {
        new SecretRepository(tx).removeUsage(row.tokenSecretId, "account", id);
      }
      tx.delete(account).where(eq(account.id, id)).run();
      return {
        removed: true,
        freedSecretId: row.tokenSecretId,
        detachedProviderIds: detached.map((entry) => entry.id),
      };
    });
  }
}
