import type { ReactNode } from "react";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import TextInput from "../atoms/TextInput";

interface EntityListSearch {
  readonly value: string;
  readonly label: string;
  readonly placeholder: string;
  readonly onChange: (value: string) => void;
}

export interface EntityListPanelProps {
  readonly heading: string;
  readonly search?: EntityListSearch;
  readonly children: ReactNode;
}

export default function EntityListPanel({ heading, search, children }: EntityListPanelProps) {
  return (
    <div className="flex min-h-0 w-65.5 flex-none flex-col gap-2 border-r border-line py-3.5 pr-2.5 pl-3">
      <div className="flex-none px-0.5 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
        {heading}
      </div>
      {search === undefined ? null : (
        <TextInput
          preset="search"
          aria-label={search.label}
          placeholder={search.placeholder}
          value={search.value}
          onChange={(event) => {
            search.onChange(event.target.value);
          }}
        />
      )}
      <ScrollArea className="flex min-h-0 flex-1 flex-col gap-2 pr-0.5">{children}</ScrollArea>
    </div>
  );
}
