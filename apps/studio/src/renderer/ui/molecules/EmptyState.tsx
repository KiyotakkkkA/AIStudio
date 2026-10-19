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
      className="max-w-110 border-0 bg-transparent px-6 py-10 text-center shadow-none"
      icon={
        <span className="mx-auto flex size-12 items-center justify-center rounded-card bg-accent-soft text-accent-medium">
          <Icon path={icon} size={22} />
        </span>
      }
      title={
        <span className="text-[15px] font-semibold tracking-[-0.01em] text-main-50">{title}</span>
      }
      description={
        <span className="block text-[13px] leading-relaxed text-main-400">{description}</span>
      }
      action={action}
    />
  );
}
