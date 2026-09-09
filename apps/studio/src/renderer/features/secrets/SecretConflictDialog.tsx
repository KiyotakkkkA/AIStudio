import ConfirmDialog from "../../ui/molecules/ConfirmDialog";
import { consumerLabel } from "./secretPresentation";
import type { SecretConflict } from "./SecretStore";

export interface SecretConflictDialogProps {
  readonly conflict: SecretConflict | null;
  readonly onClose: () => void;
}

export default function SecretConflictDialog({ conflict, onClose }: SecretConflictDialogProps) {
  return (
    <ConfirmDialog
      open={conflict !== null}
      title="Секрет используется"
      cancelLabel="Понятно"
      onCancel={onClose}
    >
      <p>
        {conflict === null ? null : `«${conflict.name}» нельзя удалить, пока на него ссылаются:`}
      </p>
      <ul className="mt-[10px] flex flex-col gap-[6px]">
        {(conflict?.consumers ?? []).map((consumer) => (
          <li
            key={`${consumer.kind}:${consumer.id}`}
            className="rounded-[6px] border border-main-700 bg-main-800 px-[10px] py-[7px] text-main-300"
          >
            <span className="text-main-100">{consumerLabel(consumer.kind)}</span>
            <span className="ml-[8px] font-mono text-[11px] text-main-500">{consumer.id}</span>
          </li>
        ))}
      </ul>
      <p className="mt-[10px]">Отвяжите секрет от этих объектов и повторите удаление.</p>
    </ConfirmDialog>
  );
}
