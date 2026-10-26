import { useRef, type KeyboardEvent, type PointerEvent } from "react";
import { AnimatePresence, motion } from "motion/react";
import { mdiBrain, mdiCreation, mdiLightningBolt } from "@mdi/js";
import type { ThinkingEffort } from "@zvs/shared";
import Icon from "../../ui/atoms/Icon";
import { useFx, useTransition } from "../../ui/motion/fx";
import { EFFORT_LABELS, EFFORT_STEPS, effortStep, stepEffort } from "./accountSetupCopy";

interface Stage {
  readonly icon: string;
  readonly fill: string;
  readonly thumb: string;
  readonly halo: string;
  readonly chip: string;
  readonly label: string;
}

const STAGES: Record<ThinkingEffort, Stage> = {
  fast: {
    icon: mdiLightningBolt,
    fill: "bg-linear-to-r from-info-soft to-info-medium",
    thumb:
      "border-info-medium text-info shadow-[0_0_0_4px_var(--color-info-soft),0_6px_16px_-4px_var(--color-info)]",
    halo: "bg-info-medium",
    chip: "bg-info-soft text-info",
    label: "text-info",
  },
  auto: {
    icon: mdiCreation,
    fill: "bg-linear-to-r from-accent-soft to-accent-medium",
    thumb:
      "border-accent-medium text-accent-medium shadow-[0_0_0_4px_var(--color-accent-soft),0_6px_16px_-4px_var(--color-accent-dark)]",
    halo: "bg-accent-medium",
    chip: "bg-accent-soft text-accent-medium",
    label: "text-accent-medium",
  },
  thinking: {
    icon: mdiBrain,
    fill: "bg-linear-to-r from-warn-soft via-warning-medium to-warning-dark",
    thumb:
      "border-warning-medium text-warning-dark shadow-[0_0_0_4px_var(--color-warn-soft),0_6px_20px_-2px_var(--color-warning-medium)]",
    halo: "bg-warning-medium",
    chip: "bg-warn-soft text-warn",
    label: "text-warn",
  },
};

const THUMB = 32;

const KEYS: Record<string, (step: number, last: number) => number> = {
  ArrowLeft: (step) => step - 1,
  ArrowDown: (step) => step - 1,
  ArrowRight: (step) => step + 1,
  ArrowUp: (step) => step + 1,
  Home: () => 0,
  End: (_, last) => last,
};

function position(index: number, last: number): string {
  return `${String(last === 0 ? 0 : (index / last) * 100)}%`;
}

export default function EffortSlider({
  label,
  value,
  steps = EFFORT_STEPS,
  disabled = false,
  onChange,
}: {
  readonly label: string;
  readonly value: ThinkingEffort;
  readonly steps?: readonly ThinkingEffort[];
  readonly disabled?: boolean;
  readonly onChange: (value: ThinkingEffort) => void;
}) {
  const rail = useRef<HTMLDivElement>(null);
  const dragging = useRef(false);
  const { effects } = useFx();
  const pop = useTransition("pop");
  const layout = useTransition("layout");
  const last = steps.length - 1;
  const step = effortStep(value, steps);
  const stage = STAGES[value];
  const percent = position(step, last);
  const lively = effects !== "none";

  const pick = (next: ThinkingEffort) => {
    if (!disabled && next !== value) onChange(next);
  };
  const fromPointer = (clientX: number) => {
    const box = rail.current?.getBoundingClientRect();
    if (!box || box.width <= 0) return;
    pick(stepEffort(((clientX - box.left - THUMB / 2) / box.width) * last, steps));
  };
  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (disabled) return;
    dragging.current = true;
    event.currentTarget.setPointerCapture(event.pointerId);
    fromPointer(event.clientX);
  };
  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    if (dragging.current) fromPointer(event.clientX);
  };
  const onPointerUp = (event: PointerEvent<HTMLDivElement>) => {
    dragging.current = false;
    if (event.currentTarget.hasPointerCapture(event.pointerId))
      event.currentTarget.releasePointerCapture(event.pointerId);
  };
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const move = KEYS[event.key];
    if (move === undefined) return;
    event.preventDefault();
    pick(stepEffort(Math.min(last, Math.max(0, move(step, last))), steps));
  };

  return (
    <div className={`min-w-0 select-none ${disabled ? "opacity-50" : ""}`}>
      <div className="mb-3 flex h-6 items-center justify-between gap-2">
        <span className="text-[12.5px] font-medium text-main-300">{label}</span>
        <span className="relative h-6 w-28 flex-none overflow-hidden">
          <AnimatePresence initial={false}>
            <motion.span
              key={value}
              initial={{ opacity: 0, y: 14 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -14 }}
              transition={pop}
              className="absolute inset-0 flex justify-end"
            >
              <span
                className={`inline-flex h-6 items-center gap-1 rounded-pill px-2.5 text-[11.5px] font-semibold ${stage.chip}`}
              >
                <Icon path={stage.icon} size={13} />
                {EFFORT_LABELS[value]}
              </span>
            </motion.span>
          </AnimatePresence>
        </span>
      </div>
      <div
        role="slider"
        tabIndex={disabled ? -1 : 0}
        aria-label={label}
        aria-valuemin={0}
        aria-valuemax={last}
        aria-valuenow={step}
        aria-valuetext={EFFORT_LABELS[value]}
        aria-disabled={disabled}
        onKeyDown={onKeyDown}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerUp}
        className={`relative h-11 touch-none rounded-full border border-line bg-main-800 shadow-[inset_0_2px_5px_rgb(0_0_0/0.2)] outline-none focus-visible:ring-2 focus-visible:ring-accent-line ${
          disabled ? "cursor-default" : "cursor-pointer"
        }`}
      >
        <div ref={rail} className="absolute top-1.25 bottom-1.25 left-1.25 right-[37px]">
          <motion.div
            aria-hidden="true"
            initial={false}
            animate={{ width: percent }}
            transition={layout}
            className={`absolute inset-y-0 left-0 box-content overflow-hidden rounded-full pr-8 transition-colors duration-300 ${stage.fill}`}
          >
            {lively && value === "auto" && (
              <span className="absolute inset-y-0 left-0 block w-2/5 animate-zvs-sweep bg-linear-to-r from-transparent via-white/30 to-transparent" />
            )}
          </motion.div>
          {steps.map((stop, index) => (
            <span
              key={stop}
              aria-hidden="true"
              style={{ left: `calc(${position(index, last)} + ${String(THUMB / 2)}px)` }}
              className={`absolute top-1/2 size-1.5 -translate-x-1/2 -translate-y-1/2 rounded-full transition-colors duration-300 ${
                index <= step ? "bg-white/70" : "bg-main-500"
              }`}
            />
          ))}
          <motion.div
            aria-hidden="true"
            initial={false}
            animate={{ left: percent }}
            transition={pop}
            className="absolute top-0 size-8"
          >
            {lively && value === "thinking" && (
              <motion.span
                className={`absolute inset-0 rounded-full ${stage.halo}`}
                initial={{ opacity: 0.45, scale: 1 }}
                animate={{ opacity: 0, scale: 1.75 }}
                transition={{ duration: 1.6, repeat: Infinity, ease: "easeOut" }}
              />
            )}
            <motion.span
              whileTap={disabled ? undefined : { scale: 0.9 }}
              className={`relative block size-8 overflow-hidden rounded-full border-2 bg-main-900 transition-[color,border-color,box-shadow] duration-300 ${stage.thumb}`}
            >
              <AnimatePresence initial={false}>
                <motion.span
                  key={value}
                  initial={{ opacity: 0, scale: 0.3, rotate: -120 }}
                  animate={{ opacity: 1, scale: 1, rotate: 0 }}
                  exit={{ opacity: 0, scale: 0.3, rotate: 120 }}
                  transition={pop}
                  className="absolute inset-0 flex items-center justify-center"
                >
                  <Icon path={stage.icon} size={16} />
                </motion.span>
              </AnimatePresence>
            </motion.span>
          </motion.div>
        </div>
      </div>
      <div className="relative mt-2 h-7">
        {steps.map((stop, index) => {
          const active = stop === value;
          const edge =
            index === 0 ? "left-0" : index === last ? "right-0" : "left-1/2 -translate-x-1/2";
          return (
            <button
              key={stop}
              type="button"
              tabIndex={-1}
              disabled={disabled}
              onClick={() => pick(stop)}
              className={`absolute top-0 ${edge} h-5 px-0.5 text-[12px] transition-colors duration-300 disabled:cursor-default ${
                active
                  ? `font-semibold ${STAGES[stop].label}`
                  : "cursor-pointer text-main-500 hover:text-main-300"
              }`}
            >
              {EFFORT_LABELS[stop]}
            </button>
          );
        })}
        <span className="absolute inset-x-0 bottom-0.5 h-0.5">
          <motion.span
            aria-hidden="true"
            initial={false}
            animate={{ left: percent, x: `-${percent}` }}
            transition={layout}
            className={`absolute top-0 block h-0.5 w-6 rounded-full transition-colors duration-300 ${stage.halo}`}
          />
        </span>
      </div>
    </div>
  );
}
