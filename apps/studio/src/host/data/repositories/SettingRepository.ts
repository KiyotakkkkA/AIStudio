import { eq } from "drizzle-orm";
import { setting, type SettingEntity } from "../schema/index.ts";
import { Repository } from "./Repository.ts";

export class SettingRepository extends Repository {
  get(key: string): SettingEntity | undefined {
    return this.db.select().from(setting).where(eq(setting.key, key)).get();
  }

  all(): SettingEntity[] {
    return this.db.select().from(setting).orderBy(setting.key).all();
  }

  set(key: string, value: string, updatedAt: number): SettingEntity {
    return this.db
      .insert(setting)
      .values({ key, value, updatedAt })
      .onConflictDoUpdate({ target: setting.key, set: { value, updatedAt } })
      .returning()
      .get();
  }

  remove(key: string): void {
    this.db.delete(setting).where(eq(setting.key, key)).run();
  }
}
