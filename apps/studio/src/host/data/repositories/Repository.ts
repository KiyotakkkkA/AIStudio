import type { DatabaseHandle } from "../types.ts";

export abstract class Repository {
  protected readonly db: DatabaseHandle;

  constructor(db: DatabaseHandle) {
    this.db = db;
  }
}
