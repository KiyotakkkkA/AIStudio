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
      className="flex items-center gap-2.25 text-left disabled:opacity-60"
    >
      <span
        className={`relative h-4.5 w-8 flex-none rounded-[9px] ${
          checked ? "bg-accent-dark" : "bg-main-600"
        }`}
      >
        <span
          className={`absolute top-0.5 size-3.5 rounded-full bg-main-900 ${
            checked ? "left-4" : "left-0.5"
          }`}
        />
      </span>
      <span className="text-[12px] text-main-300">{label}</span>
    </button>
  );
}
