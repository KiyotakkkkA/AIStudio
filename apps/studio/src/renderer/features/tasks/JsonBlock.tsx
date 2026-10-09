import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import type { Json } from "@zvs/shared";

export interface JsonBlockProps {
  readonly value: Json | undefined;
  readonly empty?: string;
  readonly maxHeight?: string;
}

export default function JsonBlock({
  value,
  empty = "нет данных",
  maxHeight = "max-h-60",
}: JsonBlockProps) {
  if (value === undefined || value === null)
    return <p className="font-mono text-[11px] text-main-500">{empty}</p>;
  return (
    <ScrollArea
      orientation="both"
      className={`rounded-card border border-main-750 bg-main-900 p-2.5 ${maxHeight}`}
    >
      <pre className="m-0 font-mono text-[11px] leading-[1.7] text-main-300">
        {JSON.stringify(value, null, 2)}
      </pre>
    </ScrollArea>
  );
}
