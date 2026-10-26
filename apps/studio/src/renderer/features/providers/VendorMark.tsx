import type { AccountFamily } from "@zvs/shared";
import Icon from "../../ui/atoms/Icon";
import { ACCOUNT_VENDOR_MARKS } from "./accountPresentation";

type VendorMarkSize = "xs" | "sm" | "md" | "lg";

const SIZES: Record<VendorMarkSize, { readonly box: string; readonly icon: number }> = {
  xs: { box: "size-4.5 rounded-[5px]", icon: 10 },
  sm: { box: "size-7 rounded-control", icon: 15 },
  md: { box: "size-9 rounded-control", icon: 19 },
  lg: { box: "size-10 rounded-control", icon: 21 },
};

export interface VendorMarkProps {
  readonly family: AccountFamily;
  readonly size?: VendorMarkSize;
  readonly className?: string;
}

export default function VendorMark({ family, size = "sm", className = "" }: VendorMarkProps) {
  const mark = ACCOUNT_VENDOR_MARKS[family];
  const { box, icon } = SIZES[size];
  return (
    <span
      aria-hidden="true"
      className={`flex flex-none items-center justify-center ${box} ${mark.tile} ${className}`}
    >
      <Icon path={mark.logo} size={icon} fillRule="evenodd" />
    </span>
  );
}
