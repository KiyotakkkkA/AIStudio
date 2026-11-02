import type { IntegrationKind } from "@zvs/shared";
import Icon from "../../ui/atoms/Icon";
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
  readonly size?: PluginMarkSize;
  readonly className?: string;
}

export default function PluginMark({ kind, size = "sm", className = "" }: PluginMarkProps) {
  const mark = PLUGIN_MARKS[kind];
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
