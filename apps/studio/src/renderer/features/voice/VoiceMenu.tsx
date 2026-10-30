import { useEffect } from "react";
import { observer } from "mobx-react-lite";
import { mdiChevronUp, mdiLoading, mdiMicrophone, mdiMicrophoneOutline } from "@mdi/js";
import { Dropdown, Tooltip } from "@kiyotakkkka/zvs-uikit-lib";
import Button from "../../ui/atoms/buttons/Button";
import IconButton from "../../ui/atoms/buttons/IconButton";
import Icon from "../../ui/atoms/Icon";
import {
  VOICE_CANCEL_HINT,
  VOICE_DEVICE_ROW,
  VOICE_INPUT,
  VOICE_MODEL_MISSING,
  VOICE_MODEL_ROW,
  VOICE_NOT_READY,
  VOICE_OPTIONS,
  VOICE_RECOGNIZING,
  VOICE_RECORD,
  VOICE_SETUP,
  VOICE_STOP,
} from "./voiceCopy";
import type VoiceStore from "./VoiceStore";

export interface VoiceBinding {
  readonly store: VoiceStore;
  readonly setup: () => void;
}

export interface VoiceMenuProps extends VoiceBinding {
  readonly disabled: boolean;
  readonly onText: (text: string) => void;
}

function InfoRow({ label, value }: { readonly label: string; readonly value: string }) {
  return (
    <div className="flex min-w-0 flex-col gap-0.5">
      <span className="text-[12px] text-main-500">{label}</span>
      <span className="truncate text-[13px] font-medium text-main-100">{value}</span>
    </div>
  );
}

function VoiceMenu({ store: voice, setup, disabled, onText }: VoiceMenuProps) {
  useEffect(() => {
    void voice.load();
    return voice.dispose;
  }, [voice]);

  const recording = voice.phase === "recording";
  useEffect(() => {
    if (!recording) return;
    const cancel = (event: KeyboardEvent) => {
      if (event.key === "Escape") void voice.cancelRecording();
    };
    window.addEventListener("keydown", cancel);
    return () => window.removeEventListener("keydown", cancel);
  }, [recording, voice]);

  const recognizing = voice.phase === "recognizing";
  const notice = voice.recordError ?? (voice.speechReady ? null : VOICE_NOT_READY);

  return (
    <Dropdown
      menuWidth={300}
      menuPlacement="top-right"
      onOpenChange={(open) => {
        if (!open) return;
        void voice.refresh();
        void voice.loadModels();
      }}
    >
      <div
        className={`flex h-8 items-center rounded-control ${
          recording
            ? "zvs-press bg-danger-medium text-white hover:bg-danger-dark"
            : "text-main-400 hover:bg-main-700"
        }`}
      >
        <Dropdown.Render>
          {({ openMenu }) => (
            <IconButton
              size="md"
              iconSize={17}
              toneClassName={recording ? "" : "hover:text-main-50"}
              path={recognizing ? mdiLoading : recording ? mdiMicrophone : mdiMicrophoneOutline}
              className={recognizing ? "[&>svg]:animate-spin" : ""}
              label={recognizing ? VOICE_RECOGNIZING : recording ? VOICE_STOP : VOICE_RECORD}
              aria-pressed={recording}
              disabled={voice.phase === "starting" || recognizing || (disabled && !recording)}
              onClick={() => {
                if (recording) {
                  void voice.stopRecording().then(onText);
                  return;
                }
                if (!voice.speechReady) {
                  openMenu();
                  return;
                }
                void voice.startRecording().then(() => {
                  if (voice.recordError !== null) openMenu();
                });
              }}
            />
          )}
        </Dropdown.Render>
        <span aria-hidden="true" className="h-4 w-px bg-current opacity-25" />
        <Tooltip label={VOICE_OPTIONS} placement="top-center" className="">
          <Dropdown.Trigger
            rounded=""
            className={`h-8 w-5 flex-none cursor-auto justify-center gap-0 border-none bg-transparent p-0 ${
              recording ? "text-white" : "text-main-400 hover:text-main-50"
            }`}
            icon={<Icon path={mdiChevronUp} size={15} />}
          >
            <span className="sr-only">{VOICE_OPTIONS}</span>
          </Dropdown.Trigger>
        </Tooltip>
      </div>
      <Dropdown.Menu
        role="menu"
        aria-label={VOICE_INPUT}
        rounded="rounded-lg"
        className="rounded-card border-line bg-main-900 p-3 shadow-pop"
      >
        <div className="flex flex-col gap-3">
          <InfoRow label={VOICE_DEVICE_ROW.label} value={voice.selectedLabel} />
          <InfoRow
            label={VOICE_MODEL_ROW.label}
            value={voice.selectedModel?.displayName ?? VOICE_MODEL_MISSING}
          />
          {recording ? (
            <p className="m-0 text-[12px] text-main-500">{VOICE_CANCEL_HINT}</p>
          ) : notice === null ? null : (
            <p role="status" className="m-0 text-[12px] text-main-400">
              {notice}
            </p>
          )}
          <Dropdown.Render>
            {({ close }) => (
              <div className="flex justify-end">
                <Button
                  tone="secondary"
                  size="sm"
                  onClick={() => {
                    close();
                    setup();
                  }}
                >
                  {VOICE_SETUP}
                </Button>
              </div>
            )}
          </Dropdown.Render>
        </div>
      </Dropdown.Menu>
    </Dropdown>
  );
}

export default observer(VoiceMenu);
