import type { InputHTMLAttributes, ReactNode } from "react";

export type TextInputProps = Omit<InputHTMLAttributes<HTMLInputElement>, "className" | "size"> & {
  readonly mono?: boolean;
  readonly invalid?: boolean;
  readonly leading?: ReactNode;
  readonly trailing?: ReactNode;
};

export default function TextInput({
  mono = false,
  invalid = false,
  leading,
  trailing,
  ...props
}: TextInputProps) {
  return (
    <div
      className={`flex h-[34px] items-center gap-[8px] rounded-[6px] border bg-main-900 px-[10px] text-[12.5px] text-main-100 focus-within:border-accent-dark ${
        invalid ? "border-err" : "border-main-600"
      }`}
    >
      {leading}
      <input
        {...props}
        className={`min-w-0 flex-1 bg-transparent text-main-100 outline-none placeholder:text-main-500 ${
          mono ? "font-mono text-[12px]" : ""
        }`}
      />
      {trailing}
    </div>
  );
}
