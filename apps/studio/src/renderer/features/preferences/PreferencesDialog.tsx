import { observer } from "mobx-react-lite";
import { useEffect, useRef, type ReactNode, type RefObject } from "react";
import { Modal, ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import {
  mdiAccountCircleOutline,
  mdiBellOutline,
  mdiBrain,
  mdiChevronDown,
  mdiClose,
  mdiCogOutline,
  mdiHarddisk,
  mdiInformationOutline,
  mdiPackageVariantClosed,
  mdiPaletteOutline,
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
import AppLogo from "../../ui/atoms/AppLogo";
import Reveal from "../../ui/motion/Reveal";
import { MeterBar, StackedBar, percent } from "../../ui/molecules/StackedBar";
import ActivePill from "../../ui/motion/ActivePill";
import FadeIn from "../../ui/motion/FadeIn";
import { useStore } from "../../stores/useStore";
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
  DEVICE_LOADING,
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
  deviceRows,
  fontSizeLabel,
  type InfoRow,
  type ToggleRowCopy,
} from "./preferencesPresentation";
import PreferencesSearch from "./PreferencesSearch";
import ThemePalettes from "./ThemePalettes";
import ComponentsTab from "../toolchain/ComponentsTab";
import ModelFields, { availableModelOptions } from "../chat/ModelFields";
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
import { DISABLED_NOTE, ENABLED_ROW, MODEL_ROW } from "../memory/memoryCopy";
import { aboutSettingId, SETTING_ENTRIES, SETTING_IDS } from "./settingsIndex";
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
};

const SettingRow = observer(function SettingRow({
  id,
  label,
  hint,
  error,
  stacked = false,
  footer,
  children,
}: {
  readonly id: string;
  readonly label: string;
  readonly hint: string;
  readonly error?: string | undefined;
  readonly stacked?: boolean;
  readonly footer?: ReactNode;
  readonly children: ReactNode;
}) {
  const { ui } = useStore();
  return (
    <div
      data-setting={id}
      className={`-mx-3 flex flex-wrap ${stacked ? "flex-col items-stretch" : "items-center"} gap-x-6 gap-y-2.5 rounded-control border-b [clip-path:inset(0_round_12px)] border-line px-3 py-4 transition-colors duration-500 last:border-b-0 ${
        ui.preferenceTarget === id ? "bg-accent-soft" : ""
      }`}
    >
      <div className="flex min-w-56 flex-1 flex-col gap-0.5">
        <span className="text-[13px] font-medium text-main-100">{label}</span>
        <span className="text-[12px] text-main-500">{hint}</span>
        {error === undefined ? null : (
          <span role="alert" className="text-[12px] text-err">
            {error}
          </span>
        )}
      </div>
      <div className={stacked ? "flex flex-col gap-2.5" : "flex flex-none items-center"}>
        {children}
      </div>
      {footer === undefined ? null : <div className="basis-full">{footer}</div>}
    </div>
  );
});

const InfoTableRows = observer(function InfoTableRows({
  rows,
}: {
  readonly rows: readonly InfoRow[];
}) {
  const { ui } = useStore();
  return (
    <dl className="m-0 grid grid-cols-[minmax(10rem,auto)_1fr] gap-x-6 gap-y-2.5 text-[13px]">
      {rows.map((row) => {
        const id = aboutSettingId(row.label);
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
    <div className="flex flex-col">
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
        <div className="ml-4 border-line pl-4">
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
        </div>
      </Reveal>
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
      error={profile.errors.toolCalls}
    >
      <div className="w-24">
        <TextInput
          aria-label={TOOL_CALLS_ROW.label}
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
            onProviderChange={execPolicy.selectJudgeProvider}
            modelTitle={JUDGE_MODEL}
            modelLabel={JUDGE_MODEL_LABEL}
            modelValue={execPolicy.judgeModelId ?? ""}
            modelOptions={availableModelOptions(execPolicy.judgeProvider)}
            modelDisabled={execPolicy.judgeProvider === undefined}
            onModelChange={execPolicy.setJudgeModel}
            placeholder={JUDGE_NONE}
          />
        </div>
        <Button
          tone="ghost"
          disabled={execPolicy.judgeProviderId === null}
          onClick={execPolicy.clearJudge}
        >
          {JUDGE_CLEAR}
        </Button>
      </div>
    </SettingRow>
  );
});

const GeneralSection = observer(function GeneralSection() {
  const { appSettings, profile, execPolicy } = useStore();
  useEffect(() => {
    void appSettings.load();
    void profile.load();
    void execPolicy.load();
  }, [appSettings, profile, execPolicy]);
  return (
    <div className="flex flex-col">
      {GENERAL_ROWS.map((row) => (
        <ToggleSettingRow key={row.key} row={row} />
      ))}
      <ToolCallsRow />
      <JudgeModelRow />
    </div>
  );
});

const ProfileSection = observer(function ProfileSection() {
  const { profile } = useStore();
  useEffect(() => {
    void profile.load();
  }, [profile]);
  return (
    <div className="flex flex-col">
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
    </div>
  );
});

const NotificationsSection = observer(function NotificationsSection() {
  const { appSettings } = useStore();
  useEffect(() => {
    void appSettings.load();
  }, [appSettings]);
  return (
    <div className="flex flex-col">
      <ToggleSettingRow row={NOTIFICATIONS_ROW} />
      {appSettings.values[NOTIFICATIONS_ROW.key] ? (
        <div className="ml-1 flex flex-col pl-5">
          {NOTIFICATION_KIND_ROWS.map((row) => (
            <ToggleSettingRow key={row.key} row={row} />
          ))}
        </div>
      ) : null}
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
    <div className="flex flex-col">
      <ToggleSettingRow row={ENABLED_ROW} />
      {enabled ? (
        <div className="ml-1 flex flex-col pl-5">
          <ToggleSettingRow row={MODEL_ROW} />
        </div>
      ) : (
        <p className="m-0 pt-1 text-[12px] text-main-500">{DISABLED_NOTE}</p>
      )}
      <MemoryFacts />
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
      <section className="space-y-3.5 rounded-card border border-line bg-main-900 p-3.5 text-[12px] text-main-300">
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
      <div className="flex flex-col">
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
      </div>
    </div>
  );
});

const AboutSection = observer(function AboutSection() {
  const { about } = useStore();
  useEffect(() => {
    void about.load();
  }, [about]);
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
        <InfoTableRows rows={appRows()} />
      </section>
      <section className="flex flex-col gap-3">
        <h3 className="m-0 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
          Компьютер
        </h3>
        {about.device !== null ? (
          <InfoTableRows rows={deviceRows(about.device)} />
        ) : about.error !== null ? (
          <p role="alert" className="m-0 text-[12px] text-err">
            {about.error}
          </p>
        ) : (
          <p className="m-0 text-[12px] text-main-500">{DEVICE_LOADING}</p>
        )}
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
      className="flex h-[min(640px,88vh)] flex-row w-[min(920px,94vw)] overflow-hidden rounded-card border border-line bg-main-900 p-0 shadow-pop"
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
