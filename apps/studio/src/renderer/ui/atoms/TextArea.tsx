import type { TextareaHTMLAttributes } from "react";

export type TextAreaProps = Omit<TextareaHTMLAttributes<HTMLTextAreaElement>, "className"> & {
  readonly invalid?: boolean;
};

export default function TextArea({ invalid = false, ...props }: TextAreaProps) {
  return (
    <textarea
      {...props}
      className={`h-[64px] w-full resize-none rounded-[6px] border bg-main-900 px-[10px] py-[9px] text-[12.5px] leading-[1.45] text-main-100 outline-none placeholder:text-main-500 focus:border-accent-dark ${
        invalid ? "border-err" : "border-main-600"
      }`}
    />
  );
}
