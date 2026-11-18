import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { observer } from "mobx-react-lite";
import { useEffect } from "react";
import { useStore } from "../../stores/useStore";
import type { RailIdentity } from "../../ui/organisms/NavRailTypes";
import { plural } from "../connections/connectionPresentation";
import ActivityCalendar from "./ActivityCalendar";

const days = (count: number) => plural(count, "день", "дня", "дней");
const number = (count: number) => count.toLocaleString("ru-RU");

function duration(ms: number) {
  const seconds = Math.floor(ms / 1000);
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor(seconds / 60) % 60;
  if (hours > 0) return `${number(hours)} ч ${number(minutes)} мин`;
  if (minutes > 0) return `${number(minutes)} мин ${number(seconds % 60)} с`;
  return `${number(seconds)} с`;
}

export default observer(function ProfileActivity({
  identity,
}: {
  readonly identity: RailIdentity;
}) {
  const { activity: store } = useStore();
  useEffect(() => {
    void store.load();
  }, [store]);
  const { activity, failed, loading } = store;
  const stats = activity && [
    ["Токены за всё время", number(activity.totalTokens)],
    ["Пик за день", number(activity.peakDailyTokens)],
    ["Самый долгий ответ", duration(activity.longestTurnMs)],
    ["Самая длинная серия", days(activity.longestStreak)],
    ["Текущая серия", days(activity.currentStreak)],
  ];

  return (
    <ScrollArea className="min-h-0 flex-1">
      <div className="mx-auto flex w-full max-w-4xl flex-col gap-9 px-5 py-10 sm:px-8 sm:py-14">
        <header className="flex flex-col items-center gap-3">
          <div
            aria-hidden="true"
            className="flex size-20 items-center justify-center rounded-full bg-accent-soft text-3xl font-semibold text-accent-medium"
          >
            {identity.initials}
          </div>
          <h1 className="text-xl font-semibold text-main-50">{identity.name}</h1>
        </header>
        {stats && (
          <dl className="grid grid-cols-[repeat(auto-fit,minmax(8rem,1fr))] gap-px overflow-hidden rounded-card border border-main-750 bg-main-750">
            {stats.map(([label, value]) => (
              <div key={label} className="flex flex-col gap-1 bg-main-850 px-3 py-4">
                <dt className="text-xs text-main-400">{label}</dt>
                <dd className="font-semibold text-main-50 tabular-nums">{value}</dd>
              </div>
            ))}
          </dl>
        )}
        {activity && <ActivityCalendar activity={activity} />}
        {loading && !activity && (
          <p role="status" className="text-sm text-main-400">
            Загрузка активности…
          </p>
        )}
        {failed && (
          <div className="flex flex-wrap items-center gap-3 text-sm text-main-300">
            <p role="status">Не удалось загрузить активность.</p>
            <button
              type="button"
              onClick={() => void store.load()}
              className="rounded-control bg-main-750 px-3 py-1.5 text-main-50 hover:bg-main-700"
            >
              Повторить
            </button>
          </div>
        )}
      </div>
    </ScrollArea>
  );
});
