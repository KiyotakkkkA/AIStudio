import { useState } from "react";
import type { ChatActivityDto } from "@zvs/shared";
import SegmentedControl from "../../ui/atoms/SegmentedControl";
import { plural } from "../connections/connectionPresentation";

const DAY_MS = 86_400_000;
const MONTH = new Intl.DateTimeFormat("ru-RU", { month: "short", timeZone: "UTC" });
const DATE = new Intl.DateTimeFormat("ru-RU", { dateStyle: "long", timeZone: "UTC" });
const VIEWS = [
  { value: "daily", label: "По дням" },
  { value: "weekly", label: "По неделям" },
  { value: "cumulative", label: "Накопительно" },
] as const;
type View = (typeof VIEWS)[number]["value"];

const LEVELS = [
  "bg-main-750",
  "bg-accent-medium/25",
  "bg-accent-medium/45",
  "bg-accent-medium/70",
  "bg-accent-medium",
] as const;

function intensity(tokens: number, peak: number) {
  return tokens === 0 ? 0 : Math.ceil((tokens / peak) * 4);
}

function tokenLabel(tokens: number) {
  return plural(tokens, "токен", "токена", "токенов");
}

export default function ActivityCalendar({ activity }: { readonly activity: ChatActivityDto }) {
  const [view, setView] = useState<View>("daily");
  const { firstDay, dailyTokens } = activity;
  const offset = (firstDay / DAY_MS + 3) % 7;
  const padded = [...Array<undefined>(offset).fill(undefined), ...dailyTokens];
  const weekCount = Math.ceil(padded.length / 7);
  const dailyPeak = Math.max(...dailyTokens);
  let cumulative = 0;
  let previousMonth = "";
  const weeks = Array.from({ length: weekCount }, (_, index) => {
    const days = padded.slice(index * 7, index * 7 + 7);
    const start = firstDay + Math.max(0, index * 7 - offset) * DAY_MS;
    const tokens = days.reduce<number>((sum, value) => sum + (value ?? 0), 0);
    cumulative += tokens;
    const month = MONTH.format(start);
    const monthLabel = month === previousMonth ? "" : month;
    previousMonth = month;
    return { days, start, monthLabel, value: view === "cumulative" ? cumulative : tokens };
  });
  const peak = Math.max(...weeks.map((week) => week.value));
  const columns = { gridTemplateColumns: `repeat(${weekCount}, minmax(0, 1fr))` };

  return (
    <section
      aria-label="Активность по токенам"
      className="rounded-card border border-main-750 bg-main-850 p-4 sm:p-5"
    >
      <div className="mb-5 flex flex-wrap items-center justify-between gap-3">
        <h2 className="text-sm font-semibold text-main-50">Активность по токенам</h2>
        <SegmentedControl
          label="Представление активности"
          options={VIEWS}
          value={view}
          onChange={setView}
        />
      </div>
      <div className="overflow-x-auto">
        <div className="min-w-130">
          <div className="grid gap-0.75" style={columns}>
            {weeks.map(({ days, start, value }, index) => (
              <div
                key={start}
                className={view === "daily" ? "grid grid-rows-7 gap-0.75" : "flex h-24 items-end"}
              >
                {view === "daily" ? (
                  days.map((tokens, day) => {
                    const timestamp = firstDay + (index * 7 + day - offset) * DAY_MS;
                    return tokens === undefined ? (
                      <span key={timestamp} />
                    ) : (
                      <span
                        key={timestamp}
                        title={`${DATE.format(timestamp)} · ${tokenLabel(tokens)}`}
                        className={`aspect-square rounded-[2px] ${LEVELS[intensity(tokens, dailyPeak)]}`}
                      />
                    );
                  })
                ) : (
                  <div
                    title={`${DATE.format(start)} · ${tokenLabel(value)}`}
                    className="w-full rounded-t-[2px] bg-accent-medium/75"
                    style={{ height: peak === 0 ? 0 : `${(value / peak) * 100}%` }}
                  />
                )}
              </div>
            ))}
          </div>
          <div className="mt-2 grid gap-0.75 text-[10px] text-main-400" style={columns}>
            {weeks.map(({ start, monthLabel }) => (
              <span key={start} className="whitespace-nowrap">
                {monthLabel}
              </span>
            ))}
          </div>
        </div>
      </div>
      <div className="mt-4 flex flex-wrap items-center justify-between gap-3 text-xs text-main-400">
        <span>Даты по UTC</span>
        {view === "daily" && (
          <div className="flex items-center gap-1">
            <span className="mr-1">Меньше</span>
            {LEVELS.map((level) => (
              <span key={level} aria-hidden="true" className={`size-2.5 rounded-[2px] ${level}`} />
            ))}
            <span className="ml-1">Больше</span>
          </div>
        )}
      </div>
    </section>
  );
}
