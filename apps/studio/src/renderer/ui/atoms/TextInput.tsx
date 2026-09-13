import { InputSmall, type InputSmallProps } from "@kiyotakkkka/zvs-uikit-lib";

export type TextInputProps = Omit<InputSmallProps, "className" | "classNames" | "rounded"> & {
  readonly mono?: boolean;
  readonly invalid?: boolean;
};

export default function TextInput({ mono = false, invalid = false, ...props }: TextInputProps) {
  const input = [
    "h-[34px] bg-main-900 text-[12.5px] text-main-100",
    invalid
      ? "border-err focus-visible:border-err"
      : "border-main-600 focus-visible:border-accent-dark",
    "focus-visible:ring-0",
    mono ? "font-mono text-[12px]" : "",
  ].join(" ");

  return (
    <div className="relative flex w-full items-center">
      <InputSmall {...props} rounded="rounded-lg" className="w-full" classNames={{ input }} />
    </div>
  );
}
