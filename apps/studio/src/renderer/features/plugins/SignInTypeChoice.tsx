import { observer } from "mobx-react-lite";
import type PluginStore from "./PluginStore";
import PluginMark from "./PluginMark";

export const SIGN_IN_TYPE_HEADING = "Способ подключения";

function SignInTypeChoice({ store }: { readonly store: PluginStore }) {
  const kind = store.kind;
  if (kind === null || store.signInTypes.length < 2) return null;
  return (
    <section className="flex flex-none flex-col gap-2.5">
      <h2 className="text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
        {SIGN_IN_TYPE_HEADING}
      </h2>
      <div role="radiogroup" aria-label={SIGN_IN_TYPE_HEADING} className="flex flex-wrap gap-2">
        {store.signInTypes.map((type) => {
          const checked = store.draftType === type.id;
          return (
            <button
              key={type.id}
              type="button"
              role="radio"
              aria-checked={checked}
              onClick={() => {
                store.chooseType(type.id);
              }}
              className={`zvs-press flex h-10 items-center gap-2.5 rounded-full border pr-4 pl-1.5 text-[13px] font-medium ${
                checked
                  ? "border-transparent bg-accent-soft text-main-50"
                  : "border-line bg-main-900 text-main-300 hover:border-line-strong hover:text-main-100"
              }`}
            >
              <PluginMark kind={kind} mark={type.mark} size="sm" className="rounded-full" />
              {type.label}
            </button>
          );
        })}
      </div>
    </section>
  );
}

export default observer(SignInTypeChoice);
