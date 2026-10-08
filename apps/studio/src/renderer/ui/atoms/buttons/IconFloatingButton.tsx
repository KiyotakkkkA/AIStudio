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
          className="flex size-7 items-center justify-center rounded-control text-main-400 hover:bg-main-700 hover:text-main-50"
        >
          <Icon path={mdiInformationOutline} size={16} />
        </button>
      </Floating.Trigger>
      <Floating.Content
        className={`${className} rounded-card border border-main-750 bg-main-900 p-3.5 text-xs/relaxed  text-main-300 shadow-pop`}
      >
        {children}
      </Floating.Content>
    </Floating>
  );
};
