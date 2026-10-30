import { observer } from "mobx-react-lite";
import { mdiWeb } from "@mdi/js";
import { Dropdown } from "@kiyotakkkka/zvs-uikit-lib";
import IconDropdownButton from "../../ui/atoms/buttons/IconDropdownButton";
import Icon from "../../ui/atoms/Icon";
import Toggle from "../../ui/atoms/Toggle";
import SegmentedControl from "../../ui/atoms/SegmentedControl";
import { ACCOUNT_VENDOR_MARKS, adapterLabel } from "../providers/accountPresentation";
import {
  ACCOUNT_SETUP_UNAVAILABLE,
  EFFORT_HINTS,
  EFFORT_LABEL,
  EFFORT_LABELS,
  EFFORT_UNSUPPORTED,
  WEB_SEARCH_LABEL,
  effortSteps,
  shownEffort,
} from "./accountSetupCopy";
import type ChatStore from "./ChatStore";
import EffortSlider from "./EffortSlider";
import type { ThinkingEffort } from "@zvs/shared";

const OPENAI_EFFORT_LABELS: Record<ThinkingEffort, string> = {
  fast: "Низкое",
  auto: "Авто",
  thinking: "Высокое",
};

const AccountEffort = observer(function AccountEffort({
  store,
  labels = EFFORT_LABELS,
  hint = true,
  busy = false,
}: {
  readonly store: ChatStore;
  readonly labels?: Record<ThinkingEffort, string>;
  readonly hint?: boolean;
  readonly busy?: boolean;
}) {
  const capabilities = store.model?.capabilities;
  const reasoning = capabilities?.includes("reasoning") ?? true;
  const steps = effortSteps(capabilities);
  const effort = reasoning ? shownEffort(store.composer.effort, steps) : "fast";
  return (
    <div>
      <EffortSlider
        label={EFFORT_LABEL}
        value={effort}
        steps={steps}
        labels={labels}
        disabled={!store.canConfigure || busy || !reasoning}
        onChange={store.composer.setEffort}
      />
      {hint && (
        <p className="mt-2 h-8.5 text-[11.5px] leading-[17px] text-main-500">
          {reasoning ? EFFORT_HINTS[effort] : EFFORT_UNSUPPORTED}
        </p>
      )}
    </div>
  );
});

const QwenSetup = observer(function QwenSetup({ store }: { readonly store: ChatStore }) {
  const { composer } = store;
  return (
    <div className="space-y-4">
      <AccountEffort store={store} />
      <div className="flex items-center gap-3 rounded-control bg-main-850 p-3">
        <span
          className={`flex size-8 flex-none items-center justify-center rounded-control transition-colors duration-300 ${
            composer.webSearch ? "bg-accent-soft text-accent-medium" : "bg-main-800 text-main-400"
          }`}
        >
          <Icon path={mdiWeb} size={18} />
        </span>
        <Toggle
          id="chat-qwen-web-search"
          checked={composer.webSearch}
          disabled={!store.canConfigure}
          label={WEB_SEARCH_LABEL}
          onChange={composer.setWebSearch}
        />
      </div>
    </div>
  );
});

export default observer(function ChatAccountProviderSetup({
  store,
}: {
  readonly store: ChatStore;
}) {
  const family = store.accountFamily;
  if (family === null) return null;
  const label = `Настройки аккаунта ${adapterLabel(family)}`;
  const active = family === "qwen-web" && store.composer.webSearch;

  return (
    <Dropdown menuWidth={360} menuPlacement="top-left">
      <IconDropdownButton
        label={label}
        path={ACCOUNT_VENDOR_MARKS[family].logo}
        iconSize={16}
        className="[fill-rule:evenodd]"
        {...(active
          ? {
              toneClassName:
                "bg-accent-soft text-accent-medium hover:bg-accent-soft hover:brightness-110",
            }
          : {})}
      />
      <Dropdown.Menu
        aria-label={label}
        rounded=""
        className="rounded-card border-line bg-main-900 p-4 shadow-pop"
      >
        {family === "openai" ? (
          <div className="space-y-4">
            <AccountEffort
              store={store}
              labels={OPENAI_EFFORT_LABELS}
              hint={false}
              busy={store.changingOpenaiMode}
            />
            <p className="text-[12.5px] font-medium text-main-100">Подключение OpenAI</p>
            <fieldset disabled={!store.canConfigure || store.changingOpenaiMode}>
              <SegmentedControl
                label="Способ подключения OpenAI"
                value={store.openaiMode}
                options={[
                  { value: "direct", label: "Веб-аккаунт" },
                  { value: "codex", label: "Codex" },
                ]}
                onChange={(mode) => void store.setOpenaiMode(mode)}
              />
            </fieldset>
            <p className="text-[11.5px] leading-snug text-main-500">
              Для режима Codex требуется установленный Codex CLI.
            </p>
          </div>
        ) : family === "qwen-web" ? (
          <QwenSetup store={store} />
        ) : (
          <p className="text-[12.5px] text-main-500">{ACCOUNT_SETUP_UNAVAILABLE}</p>
        )}
      </Dropdown.Menu>
    </Dropdown>
  );
});
