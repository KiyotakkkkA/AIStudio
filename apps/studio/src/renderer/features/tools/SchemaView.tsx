import type { Json } from "@zvs/shared";
import { describeSchema } from "./schemaFields";

const INDENT = ["pl-3.5", "pl-7", "pl-10.5"] as const;

export default function SchemaView({ schema }: { readonly schema: Json }) {
  const view = describeSchema(schema);

  if (view.kind === "none")
    return (
      <div className="rounded-card border border-main-750 bg-main-850 px-3 py-2.75 text-[12px] text-main-400">
        Инструмент не принимает параметров.
      </div>
    );

  if (view.kind === "raw")
    return (
      <pre className="m-0 max-h-56 overflow-auto rounded-card border border-main-750 bg-main-850 px-3 py-2.75 font-mono text-[11px] leading-relaxed text-main-300">
        {view.raw}
      </pre>
    );

  return (
    <div
      aria-label="Входная схема"
      className="max-h-64 overflow-auto rounded-card border border-main-750 bg-main-850 px-3 py-2.75 font-mono text-[11px] leading-[1.75] text-main-300"
    >
      <div className="text-main-500">{"{"}</div>
      {view.fields.map((field) => (
        <div
          key={field.path}
          title={field.description ?? undefined}
          className={`wrap-break-word ${INDENT[Math.min(field.depth, INDENT.length - 1)] ?? ""}`}
        >
          <span className="text-accent-medium">"{field.name}"</span>
          <span className="text-main-500">: </span>
          <span>{field.type}</span>
          {field.required ? (
            <span className="text-err" aria-label="обязательное">
              {" "}
              *
            </span>
          ) : null}
          {field.defaultValue === null ? null : (
            <span className="text-main-400"> = {field.defaultValue}</span>
          )}
        </div>
      ))}
      <div className="text-main-500">{"}"}</div>
    </div>
  );
}
