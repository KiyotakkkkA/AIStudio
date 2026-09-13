import { contract } from "@zvs/shared";
import { createIpcClient } from "@zvs/ipc";

const validateResponses = typeof __ZVS_VALIDATE_IPC__ === "boolean" ? __ZVS_VALIDATE_IPC__ : true;

export const ipc = createIpcClient(contract, window.zvs, { validateResponses });
