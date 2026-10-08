import type { ReactNode } from "react";

export interface FieldProps {
  readonly label: string;
  readonly labelAction?: ReactNode;
  readonly htmlFor?: string;
  readonly required?: boolean;
  readonly optionalHint?: boolean;
  readonly help?: string;
  readonly error?: string;
  readonly children: ReactNode;
}

export default function Field({
  label,
  labelAction,
  htmlFor,
  required = false,
  optionalHint = false,
  help,
  error,
  children,
}: FieldProps) {
  return (
    <div className="min-w-0">
      <label
        htmlFor={htmlFor}
        className="mb-1.5 flex items-center text-[12px] font-medium text-main-300"
      >
        {label}
        {required ? <span className="ml-1 text-err">*</span> : null}
        {!required && optionalHint ? (
          <span className="ml-1.5 font-normal text-main-500">· опционально</span>
        ) : null}
        {labelAction}
      </label>
      {children}
      {help !== undefined && error === undefined ? (
        <p className="mt-1.5 text-[11.5px] leading-snug text-main-500">{help}</p>
      ) : null}
      {error === undefined ? null : (
        <p className="mt-1.5 text-[11.5px] leading-snug text-err">{error}</p>
      )}
    </div>
  );
}
