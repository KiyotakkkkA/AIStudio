import { EmptyState as KitEmptyState } from "@kiyotakkkka/zvs-uikit-lib";
import type { ReactNode } from "react";
import Icon from "../atoms/Icon";

export interface EmptyStateProps {
  readonly icon: string;
  readonly title: string;
  readonly description: string;
  readonly action?: ReactNode;
}

export default function EmptyState({ icon, title, description, action }: EmptyStateProps) {
  return (
    <KitEmptyState
      className="max-w-[420px] px-[24px] py-[36px] text-center"
      icon={
        <span className="mx-auto flex size-[40px] items-center justify-center rounded-[10px] bg-main-700 text-main-300">
          <Icon path={icon} size={20} />
        </span>
      }
      title={<span className="text-[14px] font-semibold text-main-100">{title}</span>}
      description={
        <span className="block text-[12px] leading-[1.6] text-main-400">{description}</span>
      }
      action={action}
    />
  );
}
