import IconButton from "../../ui/atoms/buttons/IconButton";

export default function PanelButton({
  label,
  path,
  active,
  activeClassName = "bg-main-700 text-main-50",
  onClick,
}: {
  readonly label: string;
  readonly path: string;
  readonly active: boolean;
  readonly activeClassName?: string;
  readonly onClick: () => void;
}) {
  return (
    <IconButton
      label={label}
      tooltip="bottom-center"
      aria-pressed={active}
      path={path}
      iconSize={16}
      onClick={onClick}
      {...(active ? { toneClassName: activeClassName } : {})}
    />
  );
}
