import { AUTH_MODES, type AuthMode } from "@zvs/shared";
import { AUTH_MODE_LABELS } from "./providerPresentation";

export interface AuthModeSwitchProps {
  readonly value: AuthMode;
  readonly disabledReason: (mode: AuthMode) => string | null;
  readonly onChange: (mode: AuthMode) => void;
}

export default function AuthModeSwitch({ value, disabledReason, onChange }: AuthModeSwitchProps) {
  return (
    <div role="radiogroup" aria-label="Способ авторизации" className="flex flex-col gap-1.5">
      <div className="flex gap-1 rounded-lg border border-main-700 bg-main-800 p-1">
        {AUTH_MODES.map((mode) => {
          const reason = disabledReason(mode);
          const active = value === mode;
          return (
            <button
              key={mode}
              type="button"
              role="radio"
              aria-checked={active}
              disabled={reason !== null}
              title={reason ?? undefined}
              className={`flex-1 rounded-pill px-3.5 py-1.25 text-[12px] font-medium disabled:cursor-not-allowed disabled:opacity-50 ${
                active ? "bg-main-600 text-main-50" : "text-main-400 hover:text-main-100"
              }`}
              onClick={() => {
                onChange(mode);
              }}
            >
              {AUTH_MODE_LABELS[mode]}
            </button>
          );
        })}
      </div>
      {AUTH_MODES.map((mode) => {
        const reason = disabledReason(mode);
        return reason === null ? null : (
          <p key={mode} className="text-[11px] text-main-500">
            {reason}
          </p>
        );
      })}
    </div>
  );
}
