import type { FolderAccessAnswer } from "@zvs/shared";
import { SCOPE_ALWAYS, SCOPE_CONVERSATION, SCOPE_ONCE } from "../access/accessCopy";

export const ACCESS_REQUEST_LEAD = "Запрос доступа к управлению приложениями:";
export const PATH_UNKNOWN = "Путь не определён";

export const ACCESS_ANSWERS: readonly {
  readonly answer: FolderAccessAnswer;
  readonly label: string;
  readonly tone: "primary" | "secondary" | "danger-soft";
}[] = [
  { answer: "once", label: SCOPE_ONCE, tone: "primary" },
  { answer: "conversation", label: SCOPE_CONVERSATION, tone: "secondary" },
  { answer: "always", label: SCOPE_ALWAYS, tone: "secondary" },
  { answer: "deny", label: "Запретить", tone: "danger-soft" },
];
