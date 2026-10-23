import { observer } from "mobx-react-lite";
import { useEffect, type ReactNode } from "react";
import { Modal, ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import {
  mdiBellOutline,
  mdiClose,
  mdiCogOutline,
  mdiInformationOutline,
  mdiPaletteOutline,
} from "@mdi/js";
import IconButton from "../../ui/atoms/buttons/IconButton";
import Icon from "../../ui/atoms/Icon";
import SegmentedControl from "../../ui/atoms/SegmentedControl";
import SelectInput from "../../ui/atoms/SelectInput";
import Toggle from "../../ui/atoms/Toggle";
import AppLogo from "../../ui/atoms/AppLogo";
import { useStore } from "../../stores/useStore";
import {
  FONT_FAMILIES,
  FONT_SIZES,
  PREFERENCE_SECTIONS,
  type PreferenceSection,
} from "../../stores/UiStore";
import {
  APP_EDITION,
  APP_NAME,
  APP_VERSION,
  CLOSE_PREFERENCES,
  DEVICE_LOADING,
  FONT_FAMILY_HINT,
  FONT_FAMILY_LABEL,
  FONT_FAMILY_OPTIONS,
  FONT_SIZE_HINT,
  FONT_SIZE_LABEL,
  FONT_SIZE_OPTIONS,
  GENERAL_ROWS,
  NOTIFICATION_KIND_ROWS,
  NOTIFICATIONS_ROW,
  PREFERENCES_GROUP,
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

const SECTION_ICONS: Record<PreferenceSection, string> = {
  general: mdiCogOutline,
  appearance: mdiPaletteOutline,
  notifications: mdiBellOutline,
  about: mdiInformationOutline,
};

function SettingRow({
  label,
  hint,
  error,
  children,
}: {
  readonly label: string;
  readonly hint: string;
  readonly error?: string | undefined;
  readonly children: ReactNode;
}) {
  return (
    <div className="flex flex-wrap items-center gap-x-6 gap-y-2.5 border-b border-line py-4 last:border-b-0">
      <div className="flex min-w-56 flex-1 flex-col gap-0.5">
        <span className="text-[13px] font-medium text-main-100">{label}</span>
        <span className="text-[12px] text-main-500">{hint}</span>
        {error === undefined ? null : (
          <span role="alert" className="text-[12px] text-err">
            {error}
          </span>
        )}
      </div>
      <div className="flex flex-none items-center">{children}</div>
    </div>
  );
}

function InfoTableRows({ rows }: { readonly rows: readonly InfoRow[] }) {
  return (
    <dl className="m-0 grid grid-cols-[minmax(10rem,auto)_1fr] gap-x-6 gap-y-2.5 text-[13px]">
      {rows.map((row) => (
        <div key={row.label} className="contents">
          <dt className="text-main-500">{row.label}</dt>
          <dd className="m-0 min-w-0 wrap-break-word text-main-100">{row.value}</dd>
        </div>
      ))}
    </dl>
  );
}

const AppearanceSection = observer(function AppearanceSection() {
  const { ui } = useStore();
  return (
    <div className="flex flex-col">
      <SettingRow label={THEME_LABEL} hint={THEME_HINT}>
        <SegmentedControl
          label={THEME_LABEL}
          value={ui.theme}
          options={THEME_OPTIONS}
          onChange={ui.setTheme}
        />
      </SettingRow>
      <SettingRow label={RAIL_PICKER_LABEL} hint={RAIL_PICKER_HINT}>
        <Toggle
          label={ui.railThemePicker ? "Показан" : "Скрыт"}
          checked={ui.railThemePicker}
          onChange={ui.setRailThemePicker}
        />
      </SettingRow>
      <SettingRow label={FONT_FAMILY_LABEL} hint={FONT_FAMILY_HINT}>
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
    <SettingRow label={row.label} hint={row.hint} error={appSettings.errors[row.key]}>
      <Toggle
        label={checked ? TOGGLE_ON : TOGGLE_OFF}
        checked={checked}
        disabled={appSettings.pending[row.key] !== undefined}
        onChange={(value) => void appSettings.set(row.key, value)}
      />
    </SettingRow>
  );
});

const GeneralSection = observer(function GeneralSection() {
  const { appSettings } = useStore();
  useEffect(() => {
    void appSettings.load();
  }, [appSettings]);
  return (
    <div className="flex flex-col">
      {GENERAL_ROWS.map((row) => (
        <ToggleSettingRow key={row.key} row={row} />
      ))}
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
            v{APP_VERSION} · {APP_EDITION.toLowerCase()}
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

function PreferencesDialog() {
  const { ui } = useStore();
  const section = ui.preferences;
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
        <span className="px-2.5 pb-1.5 text-[12px] text-main-500">{PREFERENCES_GROUP}</span>
        {PREFERENCE_SECTIONS.map((item) => (
          <button
            key={item}
            type="button"
            aria-current={section === item ? "page" : undefined}
            onClick={() => ui.openPreferences(item)}
            className={`flex h-8.5 items-center gap-2.5 rounded-control px-2.5 text-left text-[13px] ${
              section === item
                ? "bg-main-800 font-medium text-main-50"
                : "text-main-300 hover:bg-main-900 hover:text-main-100"
            }`}
          >
            <Icon path={SECTION_ICONS[item]} size={16} className="flex-none" />
            {SECTION_LABELS[item]}
          </button>
        ))}
      </nav>
      <div className="flex min-w-0 flex-1 flex-col">
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
          {section === "general" ? <GeneralSection /> : null}
          {section === "appearance" ? <AppearanceSection /> : null}
          {section === "notifications" ? <NotificationsSection /> : null}
          {section === "about" ? <AboutSection /> : null}
        </ScrollArea>
      </div>
    </Modal>
  );
}

export default observer(PreferencesDialog);
