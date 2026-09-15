import { Floating, type PositionAnchor } from "@kiyotakkkka/zvs-uikit-lib";
import { mdiInformationOutline } from "@mdi/js";
import type { ReactNode } from "react";
import Icon from "../Icon";

export const IconFloatingButton = ({
  label,
  children,
  className = "w-72",
  anchor = "bottom-right",
}: {
  readonly label: string;
  readonly children: ReactNode;
  readonly className?: string;
  readonly anchor?: PositionAnchor;
}) => {
  return (
    <Floating anchor={anchor}>
      <Floating.Trigger>
        <button
          type="button"
          aria-label={label}
          className="flex size-7 items-center justify-center rounded-lg text-main-400 transition-colors hover:bg-main-750 hover:text-main-100"
        >
          <Icon path={mdiInformationOutline} size={16} />
        </button>
      </Floating.Trigger>
      <Floating.Content
        className={`${className} border border-main-700 bg-main-850 p-3 text-[11px] leading-relaxed text-main-200 shadow-lg`}
      >
        {children}
      </Floating.Content>
    </Floating>
  );
};