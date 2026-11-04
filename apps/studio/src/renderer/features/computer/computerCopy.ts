import type { FolderAccessAnswer } from "@zvs/shared";
import type { ButtonTone } from "../../ui/atoms/buttons/Button";

export const ACCESS_REQUEST_TITLE = "Запрос доступа к приложениям";
export const ACCESS_REQUEST_LEAD = "Агент просит разрешения управлять приложениями:";
export const PATH_UNKNOWN = "Путь не определён";

export const ACCESS_ANSWERS: readonly {
  readonly answer: FolderAccessAnswer;
  readonly label: string;
  readonly tone: ButtonTone;
}[] = [
  { answer: "once", label: "Только для этой сессии", tone: "primary" },
  { answer: "conversation", label: "Разрешить для этого диалога", tone: "secondary" },
  { answer: "always", label: "Разрешить всегда", tone: "secondary" },
  { answer: "deny", label: "Запретить", tone: "danger-soft" },
];
