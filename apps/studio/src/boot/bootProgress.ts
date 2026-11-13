export type BootProgress = {
  step: number;
  steps: number;
  label: string;
  done: number;
  total: number;
};

export type BootView = {
  label: string;
  percent: number;
  amount: string;
};

const UNITS = ["Б", "КБ", "МБ", "ГБ", "ТБ"] as const;

export function formatSize(bytes: number): string {
  let value = Number.isFinite(bytes) && bytes > 0 ? bytes : 0;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = unit === 0 ? 0 : 1;
  const text = value.toLocaleString("ru-RU", {
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  });
  return `${text} ${UNITS[unit]}`;
}

export function describeProgress(progress: BootProgress): BootView {
  const total = Math.max(progress.total, 0);
  const done = Math.min(Math.max(progress.done, 0), total);
  const percent = total === 0 ? 100 : Math.floor((done / total) * 100);
  const label =
    progress.steps > 1
      ? `${progress.label} (шаг ${progress.step.toLocaleString("ru-RU")} из ${progress.steps.toLocaleString("ru-RU")})`
      : progress.label;
  const amount = total === 0 ? "" : `${formatSize(done)} из ${formatSize(total)}`;
  return { label, percent, amount };
}
