import type { IntegrationKind } from "@zvs/shared";
import Icon from "../../ui/atoms/Icon";
import type { VendorMarkStyle } from "../providers/accountPresentation";
import { PLUGIN_MARKS } from "./pluginPresentation";

type PluginMarkSize = "xs" | "sm" | "md" | "lg";

const SIZES: Record<PluginMarkSize, { readonly box: string; readonly icon: number }> = {
  xs: { box: "size-4.5 rounded-[5px]", icon: 11 },
  sm: { box: "size-7 rounded-control", icon: 15 },
  md: { box: "size-9 rounded-control", icon: 19 },
  lg: { box: "size-10 rounded-control", icon: 21 },
};

export interface PluginMarkProps {
  readonly kind: IntegrationKind;
  readonly mark?: VendorMarkStyle;
  readonly size?: PluginMarkSize;
  readonly className?: string;
}

export default function PluginMark({ kind, mark, size = "sm", className = "" }: PluginMarkProps) {
  const style = mark ?? PLUGIN_MARKS[kind];
  const { box, icon } = SIZES[size];
  return (
    <span
      aria-hidden="true"
      className={`flex flex-none items-center justify-center ${box} ${style.tile} ${className}`}
    >
      <Icon path={style.logo} size={icon} fillRule="evenodd" />
    </span>
  );
}
