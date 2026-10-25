import { mdiBookOpenOutline, mdiHeartPulse, mdiSitemapOutline } from "@mdi/js";
import { observer } from "mobx-react-lite";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import Icon from "../../ui/atoms/Icon";
import EmptyState from "../../ui/molecules/EmptyState";
import type ConnectionStore from "./ConnectionStore";
import {
  formatLatency,
  formatLogTime,
  formatUptime,
  LOG_LEVEL_TEXT,
  scenariosWord,
  skillsWord,
} from "./connectionPresentation";

const HEADING = "text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase";

function HealthRail({ store }: { readonly store: ConnectionStore }) {
  const health = store.selectedHealth;
  const server = store.selectedServer;

  if (server === null || store.form?.isNew === true) {
    return (
      <aside
        aria-label="Состояние сервера"
        className="flex w-71.5 flex-none flex-col border-l border-line bg-main-800"
      >
        <div className="flex flex-1 items-center justify-center p-4">
          <EmptyState
            icon={mdiHeartPulse}
            title="Нет данных"
            description="Время работы, вызовы, задержки и журнал появятся после первого подключения."
          />
        </div>
      </aside>
    );
  }

  const usedBy = store.usedBy;
  const metrics: readonly { label: string; value: string; warn?: boolean }[] = [
    { label: "Время работы", value: formatUptime(health?.uptimeMs ?? null) },
    { label: "Перезапуски за сессию", value: String(health?.restarts ?? 0) },
    { label: "Вызовы · 24 ч", value: String(health?.calls ?? 0) },
    { label: "Медианная задержка", value: formatLatency(health?.medianLatencyMs ?? null) },
    {
      label: "Ошибки · 24 ч",
      value: String(health?.errors ?? 0),
      warn: (health?.errors ?? 0) > 0,
    },
  ];
  const log = health?.recentLog ?? [];

  return (
    <aside
      aria-label="Состояние сервера"
      className="flex min-h-0 w-71.5 flex-none flex-col gap-3 border-l border-line bg-main-800 px-3.5 py-4"
    >
      <div className={HEADING}>Состояние</div>
      <div className="flex flex-none flex-col gap-2.25 rounded-card border border-line bg-main-900 p-3">
        {metrics.map((metric) => (
          <div key={metric.label} className="flex justify-between gap-2 text-[11.5px]">
            <span className="text-main-400">{metric.label}</span>
            <span className={`font-mono ${metric.warn === true ? "text-warn" : "text-main-100"}`}>
              {metric.value}
            </span>
          </div>
        ))}
      </div>

      <div className={`mt-0.5 ${HEADING}`}>Используется</div>
      {usedBy.scenarios === 0 && usedBy.skills === 0 ? (
        <div className="flex-none rounded-card bg-main-800 px-2.75 py-2.5 text-[12px] text-main-500">
          Ни сценарии, ни навыки не ссылаются на инструменты этого сервера.
        </div>
      ) : (
        <>
          {usedBy.scenarios === 0 ? null : (
            <div className="flex flex-none items-center gap-2.25 rounded-card border border-line bg-main-900 px-2.75 py-2.5">
              <Icon path={mdiSitemapOutline} size={15} className="text-main-400" />
              <span className="flex-1 text-[12px] text-main-100">
                {scenariosWord(usedBy.scenarios)}
              </span>
            </div>
          )}
          {usedBy.skills === 0 ? null : (
            <div className="flex flex-none items-center gap-2.25 rounded-card border border-line bg-main-900 px-2.75 py-2.5">
              <Icon path={mdiBookOpenOutline} size={15} className="text-main-400" />
              <span className="flex-1 text-[12px] text-main-100">{skillsWord(usedBy.skills)}</span>
            </div>
          )}
        </>
      )}

      <div className={`mt-0.5 ${HEADING}`}>Журнал</div>
      <ScrollArea className="flex min-h-40 flex-1 flex-col rounded-card border border-line bg-main-900 px-2.75 py-2.5 font-mono text-[10.5px]/[1.8]">
        {log.length === 0 ? (
          <span className="text-main-500">Сервер пока ничего не писал в этой сессии.</span>
        ) : (
          log.map((line, index) => (
            <div key={`${String(line.at)}-${String(index)}`} className={LOG_LEVEL_TEXT[line.level]}>
              <span className="text-main-500">{formatLogTime(line.at)}</span>{" "}
              <span className="wrap-break-word">{line.line}</span>
            </div>
          ))
        )}
      </ScrollArea>
    </aside>
  );
}

export default observer(HealthRail);
