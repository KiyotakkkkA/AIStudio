import type { ReactNode } from "react";

export interface FieldProps {
  readonly label: string;
  readonly htmlFor?: string;
  readonly required?: boolean;
  readonly optionalHint?: boolean;
  readonly help?: string;
  readonly error?: string;
  readonly children: ReactNode;
}

export default function Field({
  label,
  htmlFor,
  required = false,
  optionalHint = false,
  help,
  error,
  children,
}: FieldProps) {
  return (
    <div className="min-w-0">
      <label htmlFor={htmlFor} className="mb-1.5 block text-[11px] font-medium text-main-400">
        {label}
        {required ? <span className="ml-1 text-err">*</span> : null}
        {!required && optionalHint ? (
          <span className="ml-1 font-normal text-main-500">опционально</span>
        ) : null}
      </label>
      {children}
      {help !== undefined && error === undefined ? (
        <p className="mt-1.25 text-[11px] text-main-500">{help}</p>
      ) : null}
      {error === undefined ? null : <p className="mt-1.25 text-[11px] text-err">{error}</p>}
    </div>
  );
}
