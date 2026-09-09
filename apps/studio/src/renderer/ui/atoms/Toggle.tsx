export interface ToggleProps {
  readonly checked: boolean;
  readonly label: string;
  readonly onChange: (checked: boolean) => void;
  readonly id?: string;
  readonly disabled?: boolean;
}

export default function Toggle({ checked, label, onChange, id, disabled = false }: ToggleProps) {
  return (
    <button
      id={id}
      type="button"
      role="switch"
      aria-checked={checked}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className="flex items-center gap-[9px] text-left disabled:opacity-60"
    >
      <span
        className={`relative h-[18px] w-[32px] flex-none rounded-[9px] ${
          checked ? "bg-accent-dark" : "bg-main-600"
        }`}
      >
        <span
          className={`absolute top-[2px] size-[14px] rounded-full bg-main-900 ${
            checked ? "left-[16px]" : "left-[2px]"
          }`}
        />
      </span>
      <span className="text-[12px] text-main-300">{label}</span>
    </button>
  );
}
