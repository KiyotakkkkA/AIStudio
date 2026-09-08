import { v7 } from "uuid";

/** All entity identifiers originate on the host. */
export const createId = (): string => v7();
