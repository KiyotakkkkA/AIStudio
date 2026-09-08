import type { Repositories } from "./repositories/index.ts";

export interface UnitOfWork {
  readonly repositories: Repositories;
  transaction<T>(fn: (repositories: Repositories) => T): T;
}
