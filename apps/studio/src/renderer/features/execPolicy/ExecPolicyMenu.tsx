import { observer } from "mobx-react-lite";
import { useState } from "react";
import { Dropdown } from "@kiyotakkkka/zvs-uikit-lib";
import { mdiCheck, mdiCogOutline } from "@mdi/js";
import Button from "../../ui/atoms/buttons/Button";
import IconDropdownButton from "../../ui/atoms/buttons/IconDropdownButton";
import { IconFloatingButton } from "../../ui/atoms/buttons/IconFloatingButton";
import Icon from "../../ui/atoms/Icon";
import ConfirmModal from "../../ui/molecules/ConfirmModal";
import type ExecPolicyStore from "./ExecPolicyStore";
import { EXEC_MODES, type ExecMode } from "./ExecPolicyStore";
import {
  EXEC_MODE_COPY,
  EXEC_POLICY_LABEL,
  JUDGE_MISSING,
  JUDGE_SETUP,
  NO_RESTRICTIONS_ACCEPT,
  NO_RESTRICTIONS_CONFIRM,
  NO_RESTRICTIONS_CONSEQUENCE,
} from "./execPolicyCopy";

const TRIGGER_TONES: Record<ExecMode, string | undefined> = {
  "always-ask": undefined,
  "safety-ask": "bg-accent-soft text-accent-medium hover:bg-accent-soft hover:brightness-110",
  "auto-bypass": "bg-accent-soft text-accent-medium hover:bg-accent-soft hover:brightness-110",
  "no-restrictions": "bg-warn-soft text-warn hover:bg-warn-soft hover:brightness-110",
};

function ModeText({ mode, muted }: { readonly mode: ExecMode; readonly muted: boolean }) {
  const copy = EXEC_MODE_COPY[mode];
  return (
    <span
      className={`min-w-0 flex-1 text-[13px] font-medium ${muted ? "text-main-400" : "text-main-100"}`}
    >
      {copy.label}
    </span>
  );
}

function ModeInfo({ mode }: { readonly mode: ExecMode }) {
  const copy = EXEC_MODE_COPY[mode];
  return (
    <IconFloatingButton label={`О режиме «${copy.label}»`} anchor="right-center">
      <p className="mb-2 font-medium text-main-50">{copy.label}</p>
      <p>{copy.hint}</p>
    </IconFloatingButton>
  );
}

export interface ExecPolicyBinding {
  readonly store: ExecPolicyStore;
  readonly setup: () => void;
}

function ExecPolicyMenu({ store: execPolicy, setup }: ExecPolicyBinding) {
  const [confirming, setConfirming] = useState(false);
  const current = EXEC_MODE_COPY[execPolicy.mode];
  const tone = TRIGGER_TONES[execPolicy.mode];
  const label = `${EXEC_POLICY_LABEL}: ${current.label}`;

  return (
    <>
      <Dropdown menuWidth={360} menuPlacement="top-left">
        <IconDropdownButton
          label={label}
          path={current.icon}
          iconSize={16}
          {...(tone === undefined ? {} : { toneClassName: tone })}
        />
        <Dropdown.Menu
          role="menu"
          aria-label={EXEC_POLICY_LABEL}
          rounded=""
          className="overflow-hidden rounded-card border-line bg-main-900 p-0 shadow-pop"
        >
          <header className="flex items-center gap-2 border-b border-line px-3 py-2.5">
            <Icon path={current.icon} size={15} className="flex-none text-accent-medium" />
            <span className="min-w-0 flex-1 text-[13px] font-semibold text-main-50">
              {EXEC_POLICY_LABEL}
            </span>
          </header>
          <div className="flex flex-col p-1">
            {EXEC_MODES.map((mode) => {
              const copy = EXEC_MODE_COPY[mode];
              const selected = execPolicy.mode === mode;
              if (!execPolicy.isAvailable(mode)) {
                return (
                  <div
                    key={mode}
                    aria-disabled="true"
                    className="flex items-start gap-2.5 rounded-control px-2.5 py-2"
                  >
                    <Icon path={copy.icon} size={16} className="mt-0.5 flex-none text-main-500" />
                    <span className="flex min-w-0 flex-1 flex-col gap-1.5">
                      <ModeText mode={mode} muted />
                      <span className="flex items-center gap-2">
                        <span className="min-w-0 flex-1 text-[11.5px] text-warn">
                          {JUDGE_MISSING}
                        </span>
                        <Button tone="ghost" size="sm" onClick={setup}>
                          <Icon path={mdiCogOutline} size={14} />
                          {JUDGE_SETUP}
                        </Button>
                      </span>
                    </span>
                    <ModeInfo mode={mode} />
                  </div>
                );
              }
              return (
                <div
                  key={mode}
                  className={`flex items-center rounded-control pr-1 ${selected ? "bg-main-800" : ""}`}
                >
                  <Dropdown.Item
                    role="menuitemradio"
                    aria-checked={selected}
                    rounded=""
                    className="min-w-0 flex-1 rounded-control px-2.5 py-2"
                    onClick={() => {
                      if (mode === "no-restrictions" && !selected) {
                        setConfirming(true);
                        return;
                      }
                      void execPolicy.setMode(mode);
                    }}
                  >
                    <span className="flex items-center gap-2.5 text-left">
                      <Icon
                        path={copy.icon}
                        size={16}
                        className={`flex-none ${
                          mode === "no-restrictions" ? "text-warn" : "text-main-400"
                        }`}
                      />
                      <ModeText mode={mode} muted={false} />
                      <Icon
                        path={mdiCheck}
                        size={15}
                        className={`flex-none text-accent-medium ${selected ? "" : "invisible"}`}
                      />
                    </span>
                  </Dropdown.Item>
                  <ModeInfo mode={mode} />
                </div>
              );
            })}
          </div>
        </Dropdown.Menu>
      </Dropdown>
      <ConfirmModal
        open={confirming}
        title={NO_RESTRICTIONS_CONFIRM}
        content={NO_RESTRICTIONS_CONSEQUENCE}
        confirmLabel={NO_RESTRICTIONS_ACCEPT}
        onConfirm={() => {
          void execPolicy.setMode("no-restrictions");
          setConfirming(false);
        }}
        onCancel={() => setConfirming(false)}
      />
    </>
  );
}

export default observer(ExecPolicyMenu);
