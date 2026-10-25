import type { ReactNode } from "react";
import Button from "../atoms/buttons/Button";
import TextArea from "../atoms/TextArea";

export interface TextComposerProps {
  readonly label: string;
  readonly placeholder: string;
  readonly value: string;
  readonly max: number;
  readonly submitLabel: string;
  readonly canSubmit: boolean;
  readonly onChange: (value: string) => void;
  readonly onSubmit: () => void;
  readonly cancelLabel?: string;
  readonly onCancel?: () => void;
  readonly disabled?: boolean;
  readonly minRows?: number;
  readonly maxRows?: number;
  readonly autoFocus?: boolean;
  readonly leading?: ReactNode;
}

export function composerCounter(length: number, max: number): string {
  return `${length.toLocaleString("ru-RU")} / ${max.toLocaleString("ru-RU")}`;
}

export default function TextComposer({
  label,
  placeholder,
  value,
  max,
  submitLabel,
  canSubmit,
  onChange,
  onSubmit,
  cancelLabel,
  onCancel,
  disabled = false,
  minRows = 2,
  maxRows = 10,
  autoFocus = false,
  leading,
}: TextComposerProps) {
  const submit = () => {
    if (canSubmit && !disabled) onSubmit();
  };
  const nearLimit = value.length >= max * 0.9;
  return (
    <div className="flex flex-col rounded-card border border-line bg-main-850 transition-[border-color,box-shadow] focus-within:border-accent-dark focus-within:shadow-[inset_0_0_0_1px_var(--color-accent-dark)]">
      <TextArea
        noBorder
        aria-label={label}
        placeholder={placeholder}
        value={value}
        maxLength={max}
        minRows={minRows}
        maxRows={maxRows}
        autoFocus={autoFocus}
        disabled={disabled}
        onChange={(event) => onChange(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
            event.preventDefault();
            submit();
          }
          if (event.key === "Escape" && onCancel !== undefined) {
            event.preventDefault();
            onCancel();
          }
        }}
      />
      <div className="flex min-h-10 items-center gap-2 px-2 pb-2">
        {leading}
        <span
          className={`ml-auto font-mono text-[11px] ${nearLimit ? "text-warn" : "text-main-500"}`}
        >
          {composerCounter(value.length, max)}
        </span>
        {onCancel !== undefined && cancelLabel !== undefined ? (
          <Button size="sm" tone="ghost" disabled={disabled} onClick={onCancel}>
            {cancelLabel}
          </Button>
        ) : null}
        <Button size="sm" tone="primary" disabled={!canSubmit || disabled} onClick={submit}>
          {submitLabel}
        </Button>
      </div>
    </div>
  );
}
