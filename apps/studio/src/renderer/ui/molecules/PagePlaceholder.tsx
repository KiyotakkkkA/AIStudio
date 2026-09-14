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
    <div className="flex flex-1 items-center justify-center rounded-card border border-dashed border-main-750 bg-main-800/40">
      <EmptyState
        className="max-w-105 px-6 py-9 text-center"
        icon={
          <span className="mx-auto flex size-10 items-center justify-center rounded-card bg-main-700 text-main-300">
            <Icon path={icon} size={20} />
          </span>
        }
        title={<span className="text-[14px] font-semibold text-main-100">{title}</span>}
        description={
          <span className="block text-[12px] leading-[1.6] text-main-400">
            {description}
            <span className="mt-2.5 block font-mono text-[11px] text-main-500">
              Экран будет собран в {task}
            </span>
          </span>
        }
      />
    </div>
  );
}
