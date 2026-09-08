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
    <div className="flex flex-1 items-center justify-center rounded-[10px] border border-dashed border-main-600 bg-main-800/40">
      <EmptyState
        className="max-w-[420px] px-[24px] py-[36px] text-center"
        icon={
          <span className="mx-auto flex size-[40px] items-center justify-center rounded-[10px] bg-main-700 text-main-300">
            <Icon path={icon} size={20} />
          </span>
        }
        title={<span className="text-[14px] font-semibold text-main-100">{title}</span>}
        description={
          <span className="block text-[12px] leading-[1.6] text-main-400">
            {description}
            <span className="mt-[10px] block font-mono text-[11px] text-main-500">
              Экран будет собран в {task}
            </span>
          </span>
        }
      />
    </div>
  );
}
