import type { AccountFamily } from "@zvs/shared";
import { ACCOUNT_VENDOR_MARKS } from "./accountPresentation";

type VendorMarkSize = "xs" | "sm" | "md";

const SIZES: Record<VendorMarkSize, string> = {
  xs: "size-4.5 rounded-[5px] text-[10px]",
  sm: "size-7 rounded-control text-[12px]",
  md: "size-9 rounded-control text-[14px]",
};

export interface VendorMarkProps {
  readonly family: AccountFamily;
  readonly size?: VendorMarkSize;
  readonly className?: string;
}

export default function VendorMark({ family, size = "sm", className = "" }: VendorMarkProps) {
  const mark = ACCOUNT_VENDOR_MARKS[family];
  return (
    <span
      aria-hidden="true"
      data-glyph={mark.glyph}
      className={`flex flex-none items-center justify-center font-mono leading-none font-bold before:content-[attr(data-glyph)] ${SIZES[size]} ${mark.tile} ${className}`}
    />
  );
}
