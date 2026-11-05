import { Modal } from "@kiyotakkkka/zvs-uikit-lib";
import type { DashboardStatusDto } from "@zvs/shared";
import Button from "../../ui/atoms/buttons/Button";
import {
  GALLERY_APPROVE_CONFIRM,
  GALLERY_APPROVE_HINT,
  GALLERY_APPROVE_TITLE,
  GALLERY_SCRIPT_TRUNCATED,
} from "./galleryCopy";

export default function ScriptApprovalDialog({
  open,
  status,
  onCancel,
  onConfirm,
}: {
  readonly open: boolean;
  readonly status: DashboardStatusDto;
  readonly onCancel: () => void;
  readonly onConfirm: () => void;
}) {
  const pending = status.scripts.some((script) => !script.approved);
  return (
    <Modal
      open={open}
      onClose={onCancel}
      label={GALLERY_APPROVE_TITLE}
      className="flex max-h-[86vh] w-180 max-w-[94vw] flex-col overflow-hidden rounded-card border border-line bg-main-900 p-0 shadow-pop"
    >
      <div className="flex-none px-5 pt-5">
        <h2 className="text-[15px] font-semibold tracking-[-0.01em] text-main-50">
          {GALLERY_APPROVE_TITLE}
        </h2>
        <p className="mt-2 text-[13px] leading-relaxed text-main-400">{GALLERY_APPROVE_HINT}</p>
      </div>
      <div className="mt-4 flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto px-5">
        {status.scripts.map((script) => (
          <section key={script.sha256} className="flex flex-col gap-1.5">
            <div className="flex items-center gap-2 text-[12px]">
              <span className="font-mono text-main-100">{script.name}</span>
              <span className="truncate font-mono text-[11px] text-main-500" title={script.sha256}>
                {script.sha256.slice(0, 16)}
              </span>
            </div>
            <pre className="m-0 max-h-80 overflow-auto rounded-control border border-line bg-main-850 p-3 font-mono text-[12px]/5 text-main-200">
              {script.source}
            </pre>
            {script.truncated ? (
              <span className="text-[11.5px] text-main-500">{GALLERY_SCRIPT_TRUNCATED}</span>
            ) : null}
          </section>
        ))}
      </div>
      <div className="mt-5 flex flex-none justify-end gap-2 bg-main-850 px-5 py-3">
        <Button tone="secondary" onClick={onCancel}>
          Отмена
        </Button>
        {pending ? (
          <Button tone="primary" onClick={onConfirm}>
            {GALLERY_APPROVE_CONFIRM}
          </Button>
        ) : null}
      </div>
    </Modal>
  );
}
