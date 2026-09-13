import { InputBig, type InputBigProps } from "@kiyotakkkka/zvs-uikit-lib";

export type TextAreaProps = Omit<
  InputBigProps,
  "className" | "classNames" | "label" | "description" | "error" | "showCount"
> & {
  readonly invalid?: boolean;
};

export default function TextArea({ invalid = false, ...props }: TextAreaProps) {
  const textarea = [
    "h-[64px] min-h-[64px] resize-none rounded-[6px] bg-main-900 px-[10px] py-[9px]",
    "text-[12.5px] leading-[1.45] text-main-100 focus:ring-0",
    invalid ? "border-0 focus:border-0" : "border-0 focus:border-0",
  ].join(" ");

  return <InputBig {...props} rows={2} className="w-full" classNames={{ textarea }} />;
}
