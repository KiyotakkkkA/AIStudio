import { mdiCheck, mdiContentCopy, mdiOpenInNew } from "@mdi/js";
import { observer } from "mobx-react-lite";
import { useEffect, useState } from "react";
import Button from "../../ui/atoms/buttons/Button";
import IconButton from "../../ui/atoms/buttons/IconButton";
import Icon from "../../ui/atoms/Icon";
import type PluginStore from "./PluginStore";
import { formatCountdown } from "./pluginPresentation";

export const SIGN_IN_WAITING = "Ожидание входа в браузере";
export const SIGN_IN_BROWSER_HINT = "Страница входа открыта в браузере. Завершите вход там.";
export const DEVICE_CODE_HINT = "Введите этот код на странице подтверждения.";
export const COPY_CODE = "Скопировать код";
export const CODE_COPIED = "Код скопирован";
export const CODE_EXPIRES = "Код действует ещё";
export const CODE_EXPIRED = "Срок действия кода истёк";
export const CANCEL_SIGN_IN = "Отмена";

function useTicker(now: () => number, active: boolean): number {
  const [value, setValue] = useState(now);
  useEffect(() => {
    if (!active) return undefined;
    const timer = setInterval(() => {
      setValue(now());
    }, 1000);
    return () => {
      clearInterval(timer);
    };
  }, [now, active]);
  return value;
}

function SignInPanel({ store }: { readonly store: PluginStore }) {
  const signIn = store.signIn;
  const [copied, setCopied] = useState(false);
  const now = useTicker(store.now, signIn !== null && signIn.expiresAt !== null);
  if (signIn === null) return null;
  const remaining = signIn.expiresAt === null ? null : signIn.expiresAt - now;

  return (
    <section
      aria-live="polite"
      className="flex flex-none flex-col gap-3 rounded-card border border-accent-line bg-accent-soft p-4"
    >
      <div className="flex items-center gap-2.5">
        <span className="size-2 flex-none animate-pulse rounded-full bg-accent-dark" />
        <h3 className="flex-1 text-[13px] font-semibold text-main-50">{SIGN_IN_WAITING}</h3>
        <Button
          tone="secondary"
          size="sm"
          onClick={() => {
            void store.cancelSignIn();
          }}
        >
          {CANCEL_SIGN_IN}
        </Button>
      </div>

      {signIn.userCode === null ? (
        <p className="text-[12.5px] text-main-300">{SIGN_IN_BROWSER_HINT}</p>
      ) : (
        <div className="flex flex-col gap-2.5">
          <p className="text-[12.5px] text-main-300">{DEVICE_CODE_HINT}</p>
          <div className="flex items-center gap-2">
            <span className="rounded-control bg-main-900 px-4 py-2 font-mono text-[26px] leading-none font-semibold tracking-[0.18em] text-main-50 select-all">
              {signIn.userCode}
            </span>
            <IconButton
              path={copied ? mdiCheck : mdiContentCopy}
              label={copied ? CODE_COPIED : COPY_CODE}
              size="md"
              onClick={() => {
                const code = signIn.userCode;
                if (code === null) return;
                void navigator.clipboard
                  ?.writeText(code)
                  .then(() => {
                    setCopied(true);
                  })
                  .catch(() => undefined);
              }}
            />
          </div>
          {signIn.verificationUri === null ? null : (
            <span className="flex min-w-0 items-center gap-1.5 text-[12px] text-main-400">
              <Icon path={mdiOpenInNew} size={13} className="flex-none" />
              <span className="truncate font-mono select-all">{signIn.verificationUri}</span>
            </span>
          )}
          {remaining === null ? null : (
            <span
              className={`text-[12px] ${remaining > 0 ? "text-main-400" : "text-err"}`}
              role="timer"
            >
              {remaining > 0 ? `${CODE_EXPIRES} ${formatCountdown(remaining)}` : CODE_EXPIRED}
            </span>
          )}
        </div>
      )}
    </section>
  );
}

export default observer(SignInPanel);
