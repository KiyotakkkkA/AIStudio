import { EmptyState } from "@kiyotakkkka/zvs-uikit-lib";
import Icon from "../atoms/Icon";

export interface PagePlaceholderProps {
  readonly icon: string;
  readonly title: string;
  readonly description: string;
  readonly task: string;
}

export default function PagePlaceholder({ icon, title, description, task }: PagePlaceholderProps) {
  return (
    <div className="flex flex-1 items-center justify-center">
      <EmptyState
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
          <span className="block text-[13px] leading-relaxed text-main-400">
            {description}
            <span className="mt-3 inline-flex h-6 items-center rounded-full bg-main-700 px-2.5 font-mono text-[11px] text-main-500">
              Экран будет собран в {task}
            </span>
          </span>
        }
      />
    </div>
  );
}
