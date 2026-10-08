import { InputSmall, type InputSmallProps } from "@kiyotakkkka/zvs-uikit-lib";

export type TextInputProps = Omit<InputSmallProps, "className" | "classNames" | "rounded"> & {
  readonly mono?: boolean;
  readonly invalid?: boolean;
};

export default function TextInput({ mono = false, invalid = false, ...props }: TextInputProps) {
  const input = [
    "h-9 rounded-control bg-main-900 text-[13px] outline-none transition-[border-color,box-shadow] text-main-100 placeholder:text-main-500",
    "",
    invalid
      ? "border-err focus-visible:border-err focus-visible:shadow-[inset_0_0_0_1px_var(--color-err)]"
      : "border-main-750 hover:border-main-600 focus-visible:border-accent-dark focus-visible:shadow-[inset_0_0_0_1px_var(--color-accent-dark)]",
    "focus-visible:ring-0",
    mono ? "font-mono text-[12.5px]" : "",
  ].join(" ");

  return (
    <div className="relative flex w-full items-center">
      <InputSmall {...props} rounded="" className="w-full" classNames={{ input }} />
    </div>
  );
}
