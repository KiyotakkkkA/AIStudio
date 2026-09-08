import type { SVGProps } from "react";

export type IconProps = Omit<SVGProps<SVGSVGElement>, "viewBox" | "fill" | "children"> & {
  readonly path: string;
  readonly size?: number;
};

export default function Icon({ path, size = 17, width, height, ...props }: IconProps) {
  return (
    <svg
      width={width ?? size}
      height={height ?? size}
      viewBox="0 0 24 24"
      fill="currentColor"
      aria-hidden="true"
      {...props}
    >
      <path d={path} />
    </svg>
  );
}
