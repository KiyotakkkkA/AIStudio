import { InputBig, type InputBigProps } from "@kiyotakkkka/zvs-uikit-lib";

export type TextAreaProps = Omit<
  InputBigProps,
  "className" | "classNames" | "label" | "description" | "error" | "showCount"
> & {
  readonly invalid?: boolean;
  readonly noBorder?: boolean;
};

export default function TextArea({ invalid = false, noBorder = false, ...props }: TextAreaProps) {
  const textarea = [
    "h-16 min-h-16 resize-none rounded-control bg-main-900 px-3 py-2.5 outline-none transition-[border-color,box-shadow]",
    "text-[13px] leading-normal text-main-100 placeholder:text-main-500 focus:ring-0",
    noBorder
      ? "border-0 bg-transparent shadow-none focus:border-0"
      : invalid
        ? "border-err focus:border-err focus:shadow-[inset_0_0_0_1px_var(--color-err)]"
        : "border-line hover:border-line-strong focus:border-accent-dark focus:shadow-[inset_0_0_0_1px_var(--color-accent-dark)]",
  ].join(" ");

  return <InputBig {...props} rows={2} className="w-full" classNames={{ textarea }} />;
}
