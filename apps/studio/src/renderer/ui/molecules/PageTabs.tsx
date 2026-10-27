import { Switcher } from "@kiyotakkkka/zvs-uikit-lib";
import Chip from "../atoms/Chip";
import { InlineSkeleton, PAGE_LOADING } from "./LoadingSkeleton";

export interface PageTab<T extends string> {
  readonly value: T;
  readonly label: string;
  readonly count?: number | null;
  readonly disabled?: boolean;
}

export interface PageTabsProps<T extends string> {
  readonly value: T;
  readonly tabs: readonly PageTab<T>[];
  readonly onChange: (value: T) => void;
  readonly label: string;
  readonly compact?: boolean;
}

export default function PageTabs<T extends string>({
  value,
  tabs,
  onChange,
  label,
  compact = false,
}: PageTabsProps<T>) {
  return (
    <Switcher.Root
      model={value}
      onModelChange={(next) => {
        const tab = tabs.find((entry) => entry.value === next);
        if (tab !== undefined) onChange(tab.value);
      }}
      label={label}
      rounded=""
      className="min-w-0 gap-1 overflow-x-auto rounded-none border-transparent bg-transparent p-0"
    >
      {tabs.map((tab) => (
        <Switcher.Option
          key={tab.value}
          value={tab.value}
          disabled={tab.disabled}
          className={`flex-none gap-1.75 rounded-control px-3 py-0 font-medium whitespace-nowrap text-main-400 enabled:hover:bg-transparent enabled:hover:text-main-100 aria-checked:bg-main-700 aria-checked:text-main-50 ${
            compact ? "h-7 text-[12.5px]" : "h-7.5 text-[13px]"
          }`}
        >
          {tab.label}
          {tab.count === undefined ? null : tab.count === null ? (
            <InlineSkeleton label={PAGE_LOADING} className="w-5" />
          ) : (
            <Chip tone={tab.value === value ? "raised" : "neutral"}>{String(tab.count)}</Chip>
          )}
        </Switcher.Option>
      ))}
    </Switcher.Root>
  );
}
