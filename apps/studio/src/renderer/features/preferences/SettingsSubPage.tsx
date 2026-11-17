import type { ReactNode } from "react";
import { mdiArrowLeft } from "@mdi/js";
import IconButton from "../../ui/atoms/buttons/IconButton";

export default function SettingsSubPage({
  title,
  hint,
  onBack,
  backDisabled = false,
  children,
}: {
  readonly title: string;
  readonly hint: string;
  readonly onBack: () => void;
  readonly backDisabled?: boolean;
  readonly children: ReactNode;
}) {
  return (
    <section aria-label={title} className="flex flex-col gap-4">
      <div className="flex items-center gap-2">
        <IconButton
          path={mdiArrowLeft}
          label="Назад"
          size="sm"
          disabled={backDisabled}
          onClick={onBack}
        />
        <div className="flex min-w-0 flex-1 flex-col gap-0.5">
          <h3 className="m-0 text-[14px] font-semibold text-main-50">{title}</h3>
          <p className="m-0 text-[12px] text-main-500">{hint}</p>
        </div>
      </div>
      {children}
    </section>
  );
}
