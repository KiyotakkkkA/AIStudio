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
    <pre
      className={`m-0 overflow-auto rounded-card border border-main-750 bg-main-900 p-2.5 font-mono text-[11px] leading-[1.7] text-main-300 ${maxHeight}`}
    >
      {JSON.stringify(value, null, 2)}
    </pre>
  );
}
