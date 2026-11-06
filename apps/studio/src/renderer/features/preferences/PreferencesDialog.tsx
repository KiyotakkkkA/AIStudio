import { observer } from "mobx-react-lite";
import { useEffect, useRef, type ReactNode, type RefObject } from "react";
import { Modal, ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import {
  mdiAccountCircleOutline,
  mdiBellOutline,
  mdiBrain,
  mdiChevronDown,
  mdiChevronUp,
  mdiClose,
  mdiCogOutline,
  mdiConsole,
  mdiDeleteOutline,
  mdiDownload,
  mdiHarddisk,
  mdiInformationOutline,
  mdiMicrophoneOutline,
  mdiPackageVariantClosed,
  mdiPaletteOutline,
  mdiRefresh,
  mdiWeb,
} from "@mdi/js";
import Button from "../../ui/atoms/buttons/Button";
import IconButton from "../../ui/atoms/buttons/IconButton";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import SegmentedControl from "../../ui/atoms/SegmentedControl";
import SelectInput from "../../ui/atoms/SelectInput";
import TextInput from "../../ui/atoms/TextInput";
import Toggle from "../../ui/atoms/Toggle";
import TextComposer from "../../ui/molecules/TextComposer";
import PreferenceContainer from "../../ui/molecules/PreferenceContainer";
import AppLogo from "../../ui/atoms/AppLogo";
import Reveal from "../../ui/motion/Reveal";
import { MeterBar, StackedBar, percent } from "../../ui/molecules/StackedBar";
import ActivePill from "../../ui/motion/ActivePill";
import FadeIn from "../../ui/motion/FadeIn";
import { useStore } from "../../stores/useStore";
import {
  CLI_CHECKING,
  CLI_COMMAND,
  CLI_INSTALL,
  CLI_INSTALLED,
  CLI_NOT_BUNDLED,
  CLI_NOT_INSTALLED,
  CLI_SETTING_ID,
  CLI_THEME_HINT,
  CLI_THEME_LABEL,
  CLI_THEME_SETTING_ID,
  CLI_UNINSTALL,
} from "./cliCopy";
import CliThemes from "./CliThemes";
import {
  FONT_FAMILIES,
  FONT_SIZES,
  FX_LEVELS,
  FX_PRESETS,
  type PreferenceSection,
} from "../../stores/UiStore";
import {
  APP_EDITION,
  APP_NAME,
  APP_VERSION_LABEL,
  CLOSE_PREFERENCES,
  FONT_FAMILY_HINT,
  FONT_FAMILY_LABEL,
  FONT_FAMILY_OPTIONS,
  FONT_SIZE_HINT,
  FONT_SIZE_LABEL,
  FONT_SIZE_OPTIONS,
  FX_EFFECTS_HINT,
  FX_EFFECTS_LABEL,
  FX_HINT,
  FX_LABEL,
  FX_LEVEL_OPTIONS,
  FX_MOTION_HINT,
  FX_MOTION_LABEL,
  FX_OPTIONS,
  FX_OS_NOTE,
  GENERAL_ROWS,
  NOTIFICATION_KIND_ROWS,
  NOTIFICATIONS_ROW,
  PREFERENCE_GROUPS,
  PREFERENCES_TITLE,
  RAIL_PICKER_HINT,
  RAIL_PICKER_LABEL,
  SECTION_HINTS,
  SECTION_LABELS,
  THEME_HINT,
  THEME_LABEL,
  THEME_OPTIONS,
  TOGGLE_OFF,
  TOGGLE_ON,
  appRows,
  cliRows,
  fontSizeLabel,
  type InfoRow,
  type ToggleRowCopy,
} from "./preferencesPresentation";
import PreferencesSearch from "./PreferencesSearch";
import SettingRow from "./SettingRow";
import BrowserPreferences from "../browser/BrowserPreferences";
import ThemePalettes from "./ThemePalettes";
import ComponentsTab from "../toolchain/ComponentsTab";
import { SUBAGENT_CONCURRENCY_MAX, SUBAGENT_CONCURRENCY_MIN } from "@zvs/shared";
import ModelFields, { availableModelOptions } from "../chat/ModelFields";
import {
  SUBAGENT_ACCOUNT_NOTE,
  SUBAGENT_ADD,
  SUBAGENT_CONCURRENCY_ROW,
  SUBAGENT_DOWN,
  SUBAGENT_MISSING_MODEL,
  SUBAGENT_MODEL,
  SUBAGENT_MODEL_LABEL,
  SUBAGENT_MODELS_EMPTY,
  SUBAGENT_MODELS_ROW,
  SUBAGENT_NO_PROVIDERS,
  SUBAGENT_NONE,
  SUBAGENT_PROVIDER,
  SUBAGENT_PROVIDER_LABEL,
  SUBAGENT_REMOVE,
  SUBAGENT_UP,
  SUBAGENTS_ROW,
} from "../subagents/subagentCopy";
import {
  JUDGE_CLEAR,
  JUDGE_MODEL,
  JUDGE_MODEL_LABEL,
  JUDGE_NO_PROVIDERS,
  JUDGE_NONE,
  JUDGE_PROVIDER,
  JUDGE_PROVIDER_LABEL,
  JUDGE_ROW,
} from "../execPolicy/execPolicyCopy";
import {
  STORAGE_BREAKDOWN,
  STORAGE_CATEGORY_FILLS,
  STORAGE_CATEGORY_LABELS,
  STORAGE_CLEAR,
  STORAGE_CLEAR_CACHE,
  STORAGE_CLEAR_CACHE_HINT,
  STORAGE_CLEAR_CONFIRM,
  STORAGE_CLEAR_CONSEQUENCE,
  STORAGE_CLEAR_ID,
  STORAGE_LOADING,
  STORAGE_PART_FILLS,
  STORAGE_PART_LABELS,
  STORAGE_REFRESH,
  STORAGE_TITLE,
  storageSize,
} from "./storageCopy";
import MemoryFacts from "../memory/MemoryFacts";
import {
  VOICE_CHOOSE,
  VOICE_DEVICE_MISSING,
  VOICE_DEVICE_ROW,
  VOICE_DOWNLOADING,
  VOICE_INSTALL,
  VOICE_MODEL_ROW,
  VOICE_NO_MODEL,
  VOICE_NO_MODELS,
  VOICE_RECOMMENDED,
  VOICE_REFRESH,
} from "../voice/voiceCopy";
import { formatBytes } from "../downloads/downloadPresentation";
import { ENABLED_ROW, MODEL_ROW } from "../memory/memoryCopy";
import { aboutSettingId, SETTING_ENTRIES, SETTING_IDS, type AboutGroup } from "./settingsIndex";
import {
  PROFILE_ABOUT_MAX,
  PROFILE_STYLE_MAX,
  TOOL_CALLS_MAX,
  TOOL_CALLS_MIN,
} from "./ProfileStore";
import {
  ABOUT_NEW_DIALOGS,
  ABOUT_PLACEHOLDER,
  ABOUT_ROW,
  CANCEL,
  CUSTOM_STYLE_EMPTY,
  CUSTOM_STYLE_LABEL,
  CUSTOM_STYLE_PLACEHOLDER,
  SAVE,
  STYLE_DESCRIPTIONS,
  STYLE_OPTIONS,
  STYLE_ROW,
  TOOL_CALLS_COUNT_LABEL,
  TOOL_CALLS_MODE_OPTIONS,
  TOOL_CALLS_ROW,
} from "./profileCopy";

const REVEAL_MS = 1600;
const REVEAL_ATTEMPTS = 10;

const SECTION_ICONS: Record<PreferenceSection, string> = {
  general: mdiCogOutline,
  profile: mdiAccountCircleOutline,
  appearance: mdiPaletteOutline,
  notifications: mdiBellOutline,
  memory: mdiBrain,
  data: mdiHarddisk,
  about: mdiInformationOutline,
  components: mdiPackageVariantClosed,
  cli: mdiConsole,
  voice: mdiMicrophoneOutline,
  browser: mdiWeb,
};

function NestedSettings({ children }: { readonly children: ReactNode }) {
  return (
    <div className="my-3 ml-1 rounded-r-control border-l-2 border-line bg-main-900/50 px-4 *:data-setting:py-3">
      {children}
    </div>
  );
}

const InfoTableRows = observer(function InfoTableRows({
  group,
  rows,
}: {
  readonly group: AboutGroup;
  readonly rows: readonly InfoRow[];
}) {
  const { ui } = useStore();
  return (
    <dl className="m-0 grid grid-cols-[minmax(10rem,auto)_1fr] gap-x-6 gap-y-2.5 text-[13px]">
      {rows.map((row) => {
        const id = aboutSettingId(group, row.label);
        const tone = ui.preferenceTarget === id ? "bg-accent-soft" : "";
        return (
          <div key={row.label} className="contents">
            <dt
              data-setting={id}
              className={`rounded-control transition-colors duration-500 ${tone} text-main-500`}
            >
              {row.label}
            </dt>
            <dd
              className={`m-0 min-w-0 rounded-control wrap-break-word transition-colors duration-500 ${tone} text-main-100`}
            >
              {row.value}
            </dd>
          </div>
        );
      })}
    </dl>
  );
});

const AppearanceSection = observer(function AppearanceSection() {
  const { ui } = useStore();
  return (
    <div className="flex flex-col gap-4 pt-2">
      <PreferenceContainer title="Цветовая схема">
        <SettingRow
          id={SETTING_IDS.theme}
          label={THEME_LABEL}
          hint={THEME_HINT}
          footer={<ThemePalettes />}
        >
          <SegmentedControl
            label={THEME_LABEL}
            value={ui.theme}
            options={THEME_OPTIONS}
            onChange={ui.setTheme}
          />
        </SettingRow>
        <SettingRow id={SETTING_IDS.railPicker} label={RAIL_PICKER_LABEL} hint={RAIL_PICKER_HINT}>
          <Toggle
            label={ui.railThemePicker ? "Показан" : "Скрыт"}
            checked={ui.railThemePicker}
            onChange={ui.setRailThemePicker}
          />
        </SettingRow>
      </PreferenceContainer>
      <PreferenceContainer title="Текст">
        <SettingRow id={SETTING_IDS.fontFamily} label={FONT_FAMILY_LABEL} hint={FONT_FAMILY_HINT}>
          <div className="w-52">
            <SelectInput
              label={FONT_FAMILY_LABEL}
              value={ui.fontFamily}
              options={FONT_FAMILY_OPTIONS}
              onChange={(value) => {
                const family = FONT_FAMILIES.find((option) => option === value);
                if (family !== undefined) ui.setFontFamily(family);
              }}
              menuPlacement="bottom-right"
            />
          </div>
        </SettingRow>
        <SettingRow
          id={SETTING_IDS.fontSize}
          label={FONT_SIZE_LABEL}
          hint={`${FONT_SIZE_HINT} Сейчас: ${fontSizeLabel(ui.fontSize)}.`}
        >
          <SegmentedControl
            label={FONT_SIZE_LABEL}
            value={String(ui.fontSize)}
            options={FONT_SIZE_OPTIONS}
            onChange={(value) => {
              const size = FONT_SIZES.find((option) => String(option) === value);
              if (size !== undefined) ui.setFontSize(size);
            }}
          />
        </SettingRow>
      </PreferenceContainer>
      <PreferenceContainer title="Эффекты интерфейса">
        <SettingRow
          id={SETTING_IDS.fx}
          label={FX_LABEL}
          hint={ui.fxPreset === null && ui.osReducedMotion ? `${FX_HINT} ${FX_OS_NOTE}` : FX_HINT}
        >
          <SegmentedControl
            label={FX_LABEL}
            value={ui.fxShown}
            options={FX_OPTIONS}
            onChange={(value) => {
              const preset = FX_PRESETS.find((option) => option === value);
              if (preset !== undefined) ui.setFxPreset(preset);
            }}
          />
        </SettingRow>
        <Reveal show={ui.fxPreset === "custom"}>
          <NestedSettings>
            <SettingRow id={SETTING_IDS.fxMotion} label={FX_MOTION_LABEL} hint={FX_MOTION_HINT}>
              <SegmentedControl
                label={FX_MOTION_LABEL}
                value={ui.customMotion}
                options={FX_LEVEL_OPTIONS}
                onChange={(value) => {
                  const level = FX_LEVELS.find((option) => option === value);
                  if (level !== undefined) ui.setCustomMotion(level);
                }}
              />
            </SettingRow>
            <SettingRow id={SETTING_IDS.fxEffects} label={FX_EFFECTS_LABEL} hint={FX_EFFECTS_HINT}>
              <SegmentedControl
                label={FX_EFFECTS_LABEL}
                value={ui.customEffects}
                options={FX_LEVEL_OPTIONS}
                onChange={(value) => {
                  const level = FX_LEVELS.find((option) => option === value);
                  if (level !== undefined) ui.setCustomEffects(level);
                }}
              />
            </SettingRow>
          </NestedSettings>
        </Reveal>
      </PreferenceContainer>
    </div>
  );
});

const ToggleSettingRow = observer(function ToggleSettingRow({
  row,
}: {
  readonly row: ToggleRowCopy;
}) {
  const { appSettings } = useStore();
  const checked = appSettings.values[row.key];
  return (
    <SettingRow id={row.key} label={row.label} hint={row.hint} error={appSettings.errors[row.key]}>
      <Toggle
        label={checked ? TOGGLE_ON : TOGGLE_OFF}
        checked={checked}
        disabled={appSettings.pending[row.key] !== undefined}
        onChange={(value) => void appSettings.set(row.key, value)}
      />
    </SettingRow>
  );
});

const ToolCallsRow = observer(function ToolCallsRow() {
  const { profile } = useStore();
  return (
    <SettingRow
      id={TOOL_CALLS_ROW.key}
      label={TOOL_CALLS_ROW.label}
      hint={TOOL_CALLS_ROW.hint}
      error={profile.errors.toolCalls ?? profile.errors.toolCallsUnlimited}
    >
      <div className="flex flex-col items-end gap-2.5">
        <SegmentedControl
          label={TOOL_CALLS_ROW.label}
          value={profile.toolCallsUnlimited ? "unlimited" : "limited"}
          options={TOOL_CALLS_MODE_OPTIONS}
          onChange={(mode) => void profile.setToolCallsUnlimited(mode === "unlimited")}
        />
        {profile.toolCallsUnlimited ? null : (
          <div className="w-24">
            <TextInput
              aria-label={TOOL_CALLS_COUNT_LABEL}
              mono
              type="number"
              min={TOOL_CALLS_MIN}
              max={TOOL_CALLS_MAX}
              step={1}
              value={profile.toolCallsDraft}
              disabled={profile.pending.toolCalls !== undefined}
              onChange={(event) => profile.setToolCallsDraft(event.target.value)}
              onBlur={() => void profile.commitToolCalls()}
              onKeyDown={(event) => {
                if (event.key === "Enter") event.currentTarget.blur();
              }}
            />
          </div>
        )}
      </div>
    </SettingRow>
  );
});

const JudgeModelRow = observer(function JudgeModelRow() {
  const { execPolicy } = useStore();
  const empty = execPolicy.loaded && execPolicy.providers.length === 0;
  return (
    <SettingRow
      id={JUDGE_ROW.key}
      label={JUDGE_ROW.label}
      hint={JUDGE_ROW.hint}
      stacked
      footer={
        empty ? <p className="m-0 text-[12px] text-main-500">{JUDGE_NO_PROVIDERS}</p> : undefined
      }
    >
      <div className="flex items-end gap-3">
        <div className="grid min-w-0 flex-1 grid-cols-2 gap-3">
          <ModelFields
            providerTitle={JUDGE_PROVIDER}
            providerLabel={JUDGE_PROVIDER_LABEL}
            providerValue={execPolicy.judgeProviderId ?? ""}
            providerOptions={execPolicy.providers.map((provider) => ({
              value: provider.id,
              label: provider.name,
            }))}
            providerDisabled={execPolicy.providers.length === 0}
            onProviderChange={(id) => void execPolicy.selectJudgeProvider(id)}
            modelTitle={JUDGE_MODEL}
            modelLabel={JUDGE_MODEL_LABEL}
            modelValue={execPolicy.judgeModelId ?? ""}
            modelOptions={availableModelOptions(execPolicy.judgeProvider)}
            modelDisabled={execPolicy.judgeProvider === undefined}
            onModelChange={(id) => void execPolicy.setJudgeModel(id)}
            placeholder={JUDGE_NONE}
          />
        </div>
        <Button
          tone="ghost"
          disabled={execPolicy.judgeProviderId === null}
          onClick={() => void execPolicy.clearJudge()}
        >
          {JUDGE_CLEAR}
        </Button>
      </div>
    </SettingRow>
  );
});

const CONCURRENCY_OPTIONS = Array.from(
  { length: SUBAGENT_CONCURRENCY_MAX - SUBAGENT_CONCURRENCY_MIN + 1 },
  (_, index) => {
    const value = String(SUBAGENT_CONCURRENCY_MIN + index);
    return { value, label: value };
  },
);

const SubagentsRow = observer(function SubagentsRow() {
  const { subagents } = useStore();
  const empty = subagents.loaded && subagents.providers.length === 0;
  return (
    <>
      <SettingRow
        id={SUBAGENTS_ROW.key}
        label={SUBAGENTS_ROW.label}
        hint={SUBAGENTS_ROW.hint}
        error={subagents.error ?? undefined}
      >
        <Toggle
          label={subagents.enabled ? TOGGLE_ON : TOGGLE_OFF}
          checked={subagents.enabled}
          onChange={(value) => void subagents.setEnabled(value)}
        />
      </SettingRow>
      <Reveal show={subagents.enabled}>
        <NestedSettings>
          <SettingRow
            id={SUBAGENT_MODELS_ROW.key}
            label={SUBAGENT_MODELS_ROW.label}
            hint={SUBAGENT_MODELS_ROW.hint}
            stacked
            footer={
              empty ? (
                <p className="m-0 text-[12px] text-main-500">{SUBAGENT_NO_PROVIDERS}</p>
              ) : undefined
            }
          >
            {subagents.views.length === 0 ? (
              <p className="m-0 text-[12px] text-main-400">{SUBAGENT_MODELS_EMPTY}</p>
            ) : (
              <ol className="m-0 flex list-none flex-col gap-1 p-0">
                {subagents.views.map((view, index) => (
                  <li
                    key={view.key}
                    className="flex min-w-0 items-center gap-2 rounded-control border border-line bg-main-800 px-2.5 py-1.5"
                  >
                    <span className="w-4 flex-none font-mono text-[11.5px] text-main-500">
                      {index + 1}
                    </span>
                    <span className="min-w-0 flex-1 truncate text-[12.5px] text-main-100">
                      {view.modelName}
                      <span className="text-main-500"> · {view.providerName}</span>
                    </span>
                    {view.missing ? <Chip>{SUBAGENT_MISSING_MODEL}</Chip> : null}
                    <IconButton
                      path={mdiChevronUp}
                      label={SUBAGENT_UP}
                      size="sm"
                      disabled={index === 0}
                      onClick={() => void subagents.move(index, -1)}
                    />
                    <IconButton
                      path={mdiChevronDown}
                      label={SUBAGENT_DOWN}
                      size="sm"
                      disabled={index === subagents.views.length - 1}
                      onClick={() => void subagents.move(index, 1)}
                    />
                    <IconButton
                      path={mdiClose}
                      label={SUBAGENT_REMOVE}
                      size="sm"
                      onClick={() => void subagents.remove(index)}
                    />
                  </li>
                ))}
              </ol>
            )}
            {subagents.full ? null : (
              <div className="flex flex-wrap items-end gap-3">
                <div className="grid min-w-0 flex-1 grid-cols-2 gap-3 max-sm:grid-cols-1">
                  <ModelFields
                    providerTitle={SUBAGENT_PROVIDER}
                    providerLabel={SUBAGENT_PROVIDER_LABEL}
                    providerValue={subagents.draftProviderId ?? ""}
                    providerOptions={subagents.providers.map((provider) => ({
                      value: provider.id,
                      label: provider.name,
                    }))}
                    providerDisabled={subagents.providers.length === 0}
                    onProviderChange={subagents.selectDraftProvider}
                    modelTitle={SUBAGENT_MODEL}
                    modelLabel={SUBAGENT_MODEL_LABEL}
                    modelValue={subagents.draftModelId ?? ""}
                    modelOptions={availableModelOptions(subagents.draftProvider)}
                    modelDisabled={subagents.draftProvider === undefined}
                    onModelChange={subagents.setDraftModel}
                    placeholder={SUBAGENT_NONE}
                  />
                </div>
                <Button
                  tone="ghost"
                  disabled={!subagents.canAdd}
                  onClick={() => void subagents.addDraft()}
                >
                  {SUBAGENT_ADD}
                </Button>
              </div>
            )}
          </SettingRow>
          <SettingRow
            id={SUBAGENT_CONCURRENCY_ROW.key}
            label={SUBAGENT_CONCURRENCY_ROW.label}
            hint={
              subagents.accountListed
                ? `${SUBAGENT_CONCURRENCY_ROW.hint} ${SUBAGENT_ACCOUNT_NOTE}.`
                : SUBAGENT_CONCURRENCY_ROW.hint
            }
          >
            <SegmentedControl
              label={SUBAGENT_CONCURRENCY_ROW.label}
              value={String(subagents.concurrency)}
              options={CONCURRENCY_OPTIONS}
              onChange={(value) => void subagents.setConcurrency(Number(value))}
            />
          </SettingRow>
        </NestedSettings>
      </Reveal>
    </>
  );
});

const GeneralSection = observer(function GeneralSection() {
  const { appSettings, profile, execPolicy, subagents } = useStore();
  useEffect(() => {
    void appSettings.load();
    void profile.load();
    void execPolicy.load();
    void subagents.load();
  }, [appSettings, profile, execPolicy, subagents]);
  return (
    <div className="flex flex-col gap-4 pt-2">
      <PreferenceContainer title="Работа приложения">
        {GENERAL_ROWS.map((row) => (
          <ToggleSettingRow key={row.key} row={row} />
        ))}
      </PreferenceContainer>
      <PreferenceContainer title="Выполнение действий">
        <ToolCallsRow />
        <JudgeModelRow />
      </PreferenceContainer>
      <PreferenceContainer title="Делегирование задач">
        <SubagentsRow />
      </PreferenceContainer>
    </div>
  );
});

const ProfileSection = observer(function ProfileSection() {
  const { profile } = useStore();
  useEffect(() => {
    void profile.load();
  }, [profile]);
  return (
    <div className="pt-2">
      <PreferenceContainer title="Персонализация">
        <SettingRow
          id={STYLE_ROW.key}
          label={STYLE_ROW.label}
          hint={STYLE_ROW.hint}
          error={profile.errors.style ?? profile.errors.custom}
          stacked
        >
          <div className="flex">
            <SegmentedControl
              label={STYLE_ROW.label}
              value={profile.style}
              options={STYLE_OPTIONS}
              onChange={(style) => void profile.setStyle(style)}
            />
          </div>
          <p className="m-0 text-[12px] text-main-400">{STYLE_DESCRIPTIONS[profile.style]}</p>
          {profile.style === "custom" ? (
            <>
              <TextComposer
                label={CUSTOM_STYLE_LABEL}
                placeholder={CUSTOM_STYLE_PLACEHOLDER}
                value={profile.customDraft}
                max={PROFILE_STYLE_MAX}
                submitLabel={SAVE}
                canSubmit={profile.customDirty}
                disabled={profile.pending.custom !== undefined}
                onChange={profile.setCustomDraft}
                onSubmit={() => void profile.saveCustom()}
                {...(profile.customDirty
                  ? { cancelLabel: CANCEL, onCancel: profile.resetCustom }
                  : {})}
              />
              {profile.custom.length === 0 ? (
                <p className="m-0 text-[12px] text-main-500">{CUSTOM_STYLE_EMPTY}</p>
              ) : null}
            </>
          ) : null}
        </SettingRow>
        <SettingRow
          id={ABOUT_ROW.key}
          label={ABOUT_ROW.label}
          hint={ABOUT_ROW.hint}
          error={profile.errors.about}
          stacked
        >
          <div className="flex">
            <Chip tone="accent">
              <Icon path={mdiInformationOutline} size={12} />
              {ABOUT_NEW_DIALOGS}
            </Chip>
          </div>
          <TextComposer
            label={ABOUT_ROW.label}
            placeholder={ABOUT_PLACEHOLDER}
            value={profile.aboutDraft}
            max={PROFILE_ABOUT_MAX}
            minRows={3}
            submitLabel={SAVE}
            canSubmit={profile.aboutDirty}
            disabled={profile.pending.about !== undefined}
            onChange={profile.setAboutDraft}
            onSubmit={() => void profile.saveAbout()}
            {...(profile.aboutDirty ? { cancelLabel: CANCEL, onCancel: profile.resetAbout } : {})}
          />
        </SettingRow>
      </PreferenceContainer>
    </div>
  );
});

const NotificationsSection = observer(function NotificationsSection() {
  const { appSettings } = useStore();
  useEffect(() => {
    void appSettings.load();
  }, [appSettings]);
  return (
    <div className="pt-2">
      <PreferenceContainer title="События приложения">
        <ToggleSettingRow row={NOTIFICATIONS_ROW} />
        {appSettings.values[NOTIFICATIONS_ROW.key] ? (
          <NestedSettings>
            {NOTIFICATION_KIND_ROWS.map((row) => (
              <ToggleSettingRow key={row.key} row={row} />
            ))}
          </NestedSettings>
        ) : null}
      </PreferenceContainer>
    </div>
  );
});

const MemorySection = observer(function MemorySection() {
  const { appSettings } = useStore();
  useEffect(() => {
    void appSettings.load();
  }, [appSettings]);
  const enabled = appSettings.values[ENABLED_ROW.key];
  return (
    <div className="flex flex-col gap-4 pt-2">
      <PreferenceContainer title="Управление памятью">
        <ToggleSettingRow row={ENABLED_ROW} />
        {enabled && (
          <NestedSettings>
            <ToggleSettingRow row={MODEL_ROW} />
          </NestedSettings>
        )}
      </PreferenceContainer>
      <MemoryFacts />
    </div>
  );
});

const CliSection = observer(function CliSection() {
  const { cli } = useStore();
  useEffect(() => {
    void cli.load();
  }, [cli]);
  const status = cli.status;
  const state =
    status === null
      ? CLI_CHECKING
      : !status.available
        ? CLI_NOT_BUNDLED
        : status.installed
          ? CLI_INSTALLED
          : CLI_NOT_INSTALLED;
  return (
    <div className="pt-2">
      <PreferenceContainer title="Командная строка">
        <SettingRow
          id={CLI_SETTING_ID}
          label={status?.command ?? CLI_COMMAND}
          hint={state}
          error={cli.error ?? undefined}
          footer={
            cli.notice === null ? undefined : (
              <p role="status" className="m-0 text-[12px] text-main-500">
                {cli.notice}
              </p>
            )
          }
        >
          <div className="flex items-center gap-3">
            {status?.installed ? (
              <IconButton
                path={mdiDeleteOutline}
                label={CLI_UNINSTALL}
                tone="danger"
                disabled={cli.busy}
                onClick={() => void cli.uninstall()}
              />
            ) : (
              <IconButton
                path={mdiDownload}
                label={CLI_INSTALL}
                disabled={cli.busy || !cli.available}
                onClick={() => void cli.install()}
              />
            )}
          </div>
        </SettingRow>
        <SettingRow
          id={CLI_THEME_SETTING_ID}
          label={CLI_THEME_LABEL}
          hint={CLI_THEME_HINT}
          error={cli.themeError ?? undefined}
          stacked
        >
          <CliThemes />
        </SettingRow>
      </PreferenceContainer>
    </div>
  );
});

const VoiceModelAdvice = observer(function VoiceModelAdvice() {
  const { voice } = useStore();
  const model = voice.recommended;
  if (model === undefined || voice.selectedModelRef === model.ref) return null;
  const pending =
    model.state === "downloading" ||
    model.state === "queued" ||
    (model.state === "installed" && voice.installPending);
  return (
    <div className="flex items-center gap-4 rounded-card bg-main-850 px-3.5 py-3">
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span className="text-[12px] text-main-500">{VOICE_RECOMMENDED}</span>
        <span className="text-[13px] font-medium text-main-100">
          {model.displayName} · {formatBytes(model.sizeBytes)}
        </span>
        <span className="text-[12px] text-main-500">{voice.reason}</span>
      </div>
      {pending ? (
        <span role="status" className="flex-none text-[12px] text-main-400">
          {VOICE_DOWNLOADING}
        </span>
      ) : model.state === "installed" ? (
        <Button
          tone="secondary"
          size="sm"
          disabled={voice.modelBusy}
          onClick={() => void voice.selectModel(model.ref)}
        >
          {VOICE_CHOOSE}
        </Button>
      ) : (
        <Button
          tone="primary"
          size="sm"
          disabled={voice.modelBusy}
          onClick={() => void voice.installModel(model.ref)}
        >
          {VOICE_INSTALL}
        </Button>
      )}
    </div>
  );
});

const VoiceModelRow = observer(function VoiceModelRow() {
  const { voice } = useStore();
  const installed = voice.installedModels;
  const options = [
    { value: "", label: installed.length > 0 ? VOICE_NO_MODEL : VOICE_NO_MODELS },
    ...installed.map((model) => ({ value: model.ref, label: model.displayName })),
  ];
  return (
    <SettingRow
      id={VOICE_MODEL_ROW.key}
      label={VOICE_MODEL_ROW.label}
      hint={VOICE_MODEL_ROW.hint}
      error={voice.modelError ?? undefined}
      footer={<VoiceModelAdvice />}
    >
      <div className="w-64">
        <SelectInput
          label={VOICE_MODEL_ROW.label}
          value={voice.selectedModel?.ref ?? ""}
          options={options}
          disabled={installed.length === 0 || voice.modelBusy}
          onChange={(value) => void voice.selectModel(value.length > 0 ? value : null)}
          menuPlacement="bottom-right"
        />
      </div>
    </SettingRow>
  );
});

const VoiceSection = observer(function VoiceSection() {
  const { voice } = useStore();
  useEffect(() => {
    void voice.refresh();
    void voice.loadModels();
  }, [voice]);
  const options = [
    { value: "", label: voice.systemLabel },
    ...voice.devices.map((device) => ({ value: device.id, label: device.name })),
    ...(voice.deviceId !== null && voice.selected === undefined
      ? [{ value: voice.deviceId, label: VOICE_DEVICE_MISSING }]
      : []),
  ];
  return (
    <div className="pt-2">
      <PreferenceContainer title="Распознавание речи">
        <SettingRow
          id={VOICE_DEVICE_ROW.key}
          label={VOICE_DEVICE_ROW.label}
          hint={VOICE_DEVICE_ROW.hint}
          error={voice.error ?? undefined}
        >
          <div className="flex items-center gap-2">
            <div className="w-64">
              <SelectInput
                label={VOICE_DEVICE_ROW.label}
                value={voice.deviceId ?? ""}
                options={options}
                disabled={!voice.loaded || voice.saving}
                onChange={(value) => void voice.select(value.length > 0 ? value : null)}
                menuPlacement="bottom-right"
              />
            </div>
            <IconButton
              path={mdiRefresh}
              label={VOICE_REFRESH}
              size="md"
              iconSize={17}
              disabled={voice.loading}
              onClick={() => void voice.refresh()}
            />
          </div>
        </SettingRow>
        <VoiceModelRow />
      </PreferenceContainer>
    </div>
  );
});

const DataSection = observer(function DataSection() {
  const { storage } = useStore();
  useEffect(() => {
    void storage.load();
  }, [storage]);
  const usage = storage.usage;
  const total = usage?.totalBytes ?? 0;
  const parts = storage.categories.map((item) => ({
    key: item.category,
    value: item.bytes,
    fill: STORAGE_CATEGORY_FILLS[item.category],
  }));
  return (
    <div className="flex flex-col gap-4 pt-2">
      <section className="space-y-3.5 rounded-card bg-main-850 p-3.5 text-[12px] text-main-300">
        <div className="space-y-2">
          <div className="flex items-baseline gap-2">
            <p className="m-0 flex-1 text-[13px] font-semibold text-main-50">{STORAGE_TITLE}</p>
            <span className="font-mono text-[12px] font-semibold text-main-100 tabular-nums">
              {usage === null ? STORAGE_LOADING : storageSize(total)}
            </span>
          </div>
          <StackedBar parts={parts} total={total} className="h-2" />
        </div>
        {usage === null ? null : (
          <div>
            <div className="flex items-baseline justify-between">
              <p className="m-0 font-semibold text-main-100">{STORAGE_BREAKDOWN}</p>
              <Button
                tone="ghost"
                size="sm"
                disabled={storage.loading || storage.clearing}
                onClick={() => void storage.load(true)}
              >
                {STORAGE_REFRESH}
              </Button>
            </div>
            <ul className="m-0 mt-1 list-none divide-y divide-line p-0">
              {storage.categories.map((item) => {
                const expandable = item.parts.length > 0;
                const open = expandable && storage.isExpanded(item.category);
                const row = (
                  <>
                    <span
                      className={`size-2 shrink-0 rounded-full ${STORAGE_CATEGORY_FILLS[item.category]}`}
                    />
                    <span className="min-w-0 flex-1 truncate text-main-300">
                      {STORAGE_CATEGORY_LABELS[item.category]}
                    </span>
                    {expandable ? (
                      <Icon
                        path={mdiChevronDown}
                        size={14}
                        className={`shrink-0 text-main-500 group-hover/row:text-main-200 anim-on:transition-transform ${open ? "rotate-180" : ""}`}
                      />
                    ) : null}
                    <MeterBar
                      value={item.bytes}
                      total={total}
                      fill={STORAGE_CATEGORY_FILLS[item.category]}
                      className="w-24"
                    />
                    <span className="w-16 shrink-0 text-right font-mono text-[11px] text-main-400 tabular-nums">
                      {storageSize(item.bytes)}
                    </span>
                    <span className="w-9 shrink-0 text-right font-mono text-[11px] text-main-500">
                      {percent(item.bytes, total)}
                    </span>
                  </>
                );
                return (
                  <li key={item.category}>
                    {expandable ? (
                      <button
                        type="button"
                        aria-expanded={open}
                        onClick={() => storage.toggle(item.category)}
                        className="group/row flex w-full cursor-pointer items-center gap-2 py-1.5 text-left text-[12px]"
                      >
                        {row}
                      </button>
                    ) : (
                      <div className="flex items-center gap-2 py-1.5">{row}</div>
                    )}
                    <Reveal show={open}>
                      <ul className="m-0 mb-1.5 ml-4 list-none border-l border-line p-0 pl-3">
                        {item.parts.map((part) => (
                          <li key={part.part} className="flex items-center gap-2 py-1">
                            <span
                              className={`size-1.5 shrink-0 rounded-full ${STORAGE_PART_FILLS[part.part]}`}
                            />
                            <span className="min-w-0 flex-1 truncate text-[11.5px] text-main-400">
                              {STORAGE_PART_LABELS[part.part]}
                            </span>
                            <MeterBar
                              value={part.bytes}
                              total={item.bytes}
                              fill={STORAGE_PART_FILLS[part.part]}
                              className="w-24"
                            />
                            <span className="w-16 shrink-0 text-right font-mono text-[11px] text-main-400 tabular-nums">
                              {storageSize(part.bytes)}
                            </span>
                            <span className="w-9 shrink-0 text-right font-mono text-[11px] text-main-500">
                              {percent(part.bytes, item.bytes)}
                            </span>
                          </li>
                        ))}
                      </ul>
                    </Reveal>
                  </li>
                );
              })}
            </ul>
          </div>
        )}
      </section>
      <PreferenceContainer title="Очистка данных">
        <SettingRow
          id={STORAGE_CLEAR_ID}
          label={STORAGE_CLEAR_CACHE}
          hint={STORAGE_CLEAR_CACHE_HINT}
          error={storage.error ?? undefined}
        >
          <Button
            tone="ghost"
            disabled={storage.clearing}
            needConfirm
            modalSetup={{
              title: STORAGE_CLEAR_CONFIRM,
              content: STORAGE_CLEAR_CONSEQUENCE,
              confirmLabel: STORAGE_CLEAR,
            }}
            onClick={() => void storage.clearCache()}
          >
            {STORAGE_CLEAR}
          </Button>
        </SettingRow>
        {storage.notice === null ? null : (
          <p role="status" className="m-0 pt-1 text-[12px] text-main-500">
            {storage.notice}
          </p>
        )}
      </PreferenceContainer>
    </div>
  );
});

const AboutSection = observer(function AboutSection() {
  return (
    <div className="flex flex-col gap-6 pt-2">
      <div className="flex items-center gap-3.5 rounded-card bg-main-850 px-4 py-3.5">
        <AppLogo size={40} />
        <div className="flex min-w-0 flex-col leading-tight">
          <span className="text-[15px] font-semibold tracking-[-0.01em] text-main-50">
            {APP_NAME}
          </span>
          <span className="font-mono text-[12px] text-main-500">
            {APP_VERSION_LABEL} · {APP_EDITION.toLowerCase()}
          </span>
        </div>
      </div>
      <section className="flex flex-col gap-3">
        <h3 className="m-0 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
          Приложение
        </h3>
        <InfoTableRows group="app" rows={appRows()} />
      </section>
      <section className="flex flex-col gap-3">
        <h3 className="m-0 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
          {SECTION_LABELS.cli}
        </h3>
        <InfoTableRows group="cli" rows={cliRows()} />
      </section>
    </div>
  );
});

function useRevealTarget(container: RefObject<HTMLElement | null>) {
  const { ui } = useStore();
  const target = ui.preferenceTarget;
  const section = ui.preferences;
  useEffect(() => {
    if (target === null) return;
    const entry = SETTING_ENTRIES.find((item) => item.id === target);
    let attempts = 0;
    let timer: ReturnType<typeof setTimeout>;
    const find = (id: string) =>
      container.current?.querySelector<HTMLElement>(`[data-setting="${id}"]`) ?? null;
    const reveal = () => {
      const element = find(target);
      if (element === null && entry?.fallback !== undefined && find(entry.fallback) !== null) {
        ui.revealPreference(entry.section, entry.fallback);
        return;
      }
      if (element === null) {
        attempts += 1;
        timer = setTimeout(attempts < REVEAL_ATTEMPTS ? reveal : ui.clearPreferenceTarget, 100);
        return;
      }
      element.scrollIntoView({ block: "center", behavior: "smooth" });
      timer = setTimeout(ui.clearPreferenceTarget, REVEAL_MS);
    };
    timer = setTimeout(reveal, 0);
    return () => clearTimeout(timer);
  }, [container, target, section, ui]);
}

function PreferencesDialog() {
  const { ui } = useStore();
  const section = ui.preferences;
  const content = useRef<HTMLDivElement | null>(null);
  useRevealTarget(content);
  return (
    <Modal
      open={section !== null}
      onClose={ui.closePreferences}
      label={PREFERENCES_TITLE}
      className="flex h-[min(860px,92vh)] w-[min(1240px,95vw)] flex-row overflow-hidden rounded-card border border-line bg-main-900 p-0 shadow-pop"
    >
      <nav
        aria-label={PREFERENCES_TITLE}
        className="flex w-56 flex-none flex-col gap-0.5 border-r border-line bg-main-950 px-3 py-4"
      >
        <PreferencesSearch />
        {PREFERENCE_GROUPS.map((group, index) => (
          <div key={group.label} className="flex flex-col gap-0.5">
            <span
              className={`px-2.5 pb-1.5 text-[12px] text-main-500 ${index === 0 ? "" : "pt-4"}`}
            >
              {group.label}
            </span>
            {group.sections.map((item) => (
              <button
                key={item}
                type="button"
                aria-current={section === item ? "page" : undefined}
                onClick={() => ui.openPreferences(item)}
                className={`relative flex h-8.5 items-center gap-2.5 rounded-control px-2.5 text-left text-[13px] ${
                  section === item
                    ? "font-medium text-main-50"
                    : "text-main-300 hover:bg-main-900 hover:text-main-100"
                }`}
              >
                {section === item ? (
                  <ActivePill
                    layoutId="preferences-section"
                    className="rounded-control bg-main-800"
                  />
                ) : null}
                <Icon path={SECTION_ICONS[item]} size={16} className="relative flex-none" />
                <span className="relative">{SECTION_LABELS[item]}</span>
              </button>
            ))}
          </div>
        ))}
      </nav>
      <div ref={content} className="flex min-w-0 flex-1 flex-col">
        <div className="flex flex-none items-start gap-3 px-7 pt-6 pb-2">
          <div className="flex min-w-0 flex-1 flex-col gap-0.5">
            <h2 className="m-0 text-[16px] font-semibold tracking-[-0.01em] text-main-50">
              {section === null ? PREFERENCES_TITLE : SECTION_LABELS[section]}
            </h2>
            {section === null ? null : (
              <p className="m-0 text-[12.5px] text-main-400">{SECTION_HINTS[section]}</p>
            )}
          </div>
          <IconButton
            path={mdiClose}
            label={CLOSE_PREFERENCES}
            iconSize={18}
            onClick={ui.closePreferences}
          />
        </div>
        <ScrollArea className="min-h-0 flex-1 px-7 pb-6">
          <FadeIn key={section}>
            {section === "general" ? <GeneralSection /> : null}
            {section === "profile" ? <ProfileSection /> : null}
            {section === "appearance" ? <AppearanceSection /> : null}
            {section === "notifications" ? <NotificationsSection /> : null}
            {section === "memory" ? <MemorySection /> : null}
            {section === "data" ? <DataSection /> : null}
            {section === "about" ? <AboutSection /> : null}
            {section === "cli" ? <CliSection /> : null}
            {section === "voice" ? <VoiceSection /> : null}
            {section === "browser" ? <BrowserPreferences /> : null}
            {section === "components" ? (
              <div data-setting={SETTING_IDS.components} className="pt-2">
                <ComponentsTab />
              </div>
            ) : null}
          </FadeIn>
        </ScrollArea>
      </div>
    </Modal>
  );
}

export default observer(PreferencesDialog);
