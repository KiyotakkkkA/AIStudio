import { Floating } from "@kiyotakkkka/zvs-uikit-lib";
import { mdiInformationOutline } from "@mdi/js";
import type { ReactNode } from "react";
import Icon from "./Icon";

export const InfoButton = ({
  label,
  children,
}: {
  readonly label: string;
  readonly children: ReactNode;
}) => {
  return (
    <Floating anchor="bottom-right">
      <Floating.Trigger>
        <button
          type="button"
          aria-label={label}
          className="flex size-7 items-center justify-center rounded-full text-main-400 transition-colors hover:bg-main-750 hover:text-main-100 focus-visible:outline-2 focus-visible:outline-accent-dark"
        >
          <Icon path={mdiInformationOutline} size={16} />
        </button>
      </Floating.Trigger>
      <Floating.Content className="w-72 border border-main-700 bg-main-850 p-3 text-[11px] leading-relaxed text-main-200 shadow-lg">
        {children}
      </Floating.Content>
    </Floating>
  );
};
