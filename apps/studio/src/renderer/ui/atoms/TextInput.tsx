import { InputSmall, type InputSmallProps } from "@kiyotakkkka/zvs-uikit-lib";
import type { ReactNode } from "react";

export type TextInputProps = Omit<InputSmallProps, "className" | "classNames" | "rounded"> & {
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
  const input = [
    "h-[34px] rounded-[6px] bg-main-900 px-[10px] text-[12.5px] text-main-100",
    invalid
      ? "border-err focus-visible:border-err"
      : "border-main-600 focus-visible:border-accent-dark",
    "focus-visible:ring-0",
    leading === undefined ? "" : "pl-[32px]",
    trailing === undefined ? "" : "pr-[34px]",
    mono ? "font-mono text-[12px]" : "",
  ].join(" ");

  return (
    <div className="relative flex w-full items-center">
      {leading === undefined ? null : (
        <span className="pointer-events-none absolute left-2.5 z-10 flex items-center text-main-500">
          {leading}
        </span>
      )}
      <InputSmall {...props} rounded="" className="w-full" classNames={{ input }} />
      {trailing === undefined ? null : (
        <span className="absolute right-2 z-10 flex items-center">{trailing}</span>
      )}
    </div>
  );
}
