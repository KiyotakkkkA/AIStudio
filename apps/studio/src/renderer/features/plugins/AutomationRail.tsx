import { mdiHeartPulse, mdiSitemapOutline } from "@mdi/js";
import { observer } from "mobx-react-lite";
import { useNavigate } from "react-router-dom";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import type { IntegrationActivityLevel } from "@zvs/shared";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import { ROUTES } from "../../app/routes";
import { formatLogTime, TIER_LABELS } from "../connections/connectionPresentation";
import type PluginStore from "./PluginStore";

const HEADING = "text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase";

export const RAIL_LABEL = "Автоматизация";
export const TRIGGERS_HEADING = "Триггеры";
export const TOOLS_HEADING = "Инструменты";
export const USED_BY_HEADING = "Используется в";
export const HEALTH_HEADING = "Состояние";
export const ACTIVITY_HEADING = "Журнал";
export const NO_TRIGGERS = "Плагин не объявляет триггеров.";
export const NO_TOOLS = "Инструменты появятся после настройки доступа.";
export const NOT_USED = "Ни один сценарий не запускается от этого аккаунта.";
export const NO_ACTIVITY = "Записей пока нет.";
export const NO_ACCOUNT_DATA = "Данные появятся после подключения аккаунта.";

const LEVEL_TEXT: Record<IntegrationActivityLevel, string> = {
  info: "text-main-300",
  warn: "text-warn",
  error: "text-err",
};

function AutomationRail({ store }: { readonly store: PluginStore }) {
  const navigate = useNavigate();
  const account = store.selectedAccount;
  const descriptor = store.kindDescriptor;
  const triggers = store.detail?.triggers ?? descriptor?.triggers ?? [];
  const tools =
    store.detail?.tools.map((tool) => ({ ...tool, id: tool.key })) ??
    descriptor?.tools.map((tool) => ({ ...tool, id: tool.name })) ??
    [];
  const usedBy = store.usedBy;
  const health = account === null ? null : store.healthLine(account);
  const counters = account?.counters ?? null;
  const metrics =
    counters === null
      ? []
      : [
          { label: "События", value: counters.events },
          { label: "Повторы", value: counters.duplicates },
          { label: "Запуски сценариев", value: counters.runs },
          { label: "Вызовы инструментов", value: counters.toolCalls },
          { label: "Ошибки", value: counters.errors, warn: counters.errors > 0 },
        ];

  return (
    <aside
      aria-label={RAIL_LABEL}
      className="flex min-h-0 w-71.5 flex-none flex-col border-l border-line bg-main-800"
    >
      <ScrollArea className="flex min-h-0 flex-1 flex-col gap-3 px-3.5 py-4">
        <div className={HEADING}>{TRIGGERS_HEADING}</div>
        {triggers.length === 0 ? (
          <span className="text-[12px] text-main-500">{NO_TRIGGERS}</span>
        ) : (
          <div className="flex flex-none flex-col gap-1.5">
            {triggers.map((trigger) => (
              <div
                key={trigger.name}
                title={trigger.description}
                className="flex min-w-0 flex-col gap-0.5 rounded-card border border-line bg-main-900 px-2.75 py-2"
              >
                <span className="truncate text-[12px] text-main-100">{trigger.label}</span>
                <span className="truncate font-mono text-[10.5px] text-main-500">
                  {trigger.name}
                </span>
              </div>
            ))}
          </div>
        )}

        <div className={`mt-0.5 ${HEADING}`}>{TOOLS_HEADING}</div>
        {tools.length === 0 ? (
          <span className="text-[12px] text-main-500">{NO_TOOLS}</span>
        ) : (
          <div className="flex flex-none flex-col gap-1.5">
            {tools.map((tool) => (
              <div
                key={tool.id}
                title={tool.description}
                className="flex min-w-0 items-center gap-2 rounded-card border border-line bg-main-900 px-2.75 py-2"
              >
                <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                  <span className="truncate text-[12px] text-main-100">{tool.label}</span>
                  <span className="truncate font-mono text-[10.5px] text-main-500">{tool.id}</span>
                </span>
                {tool.sideEffect ? <Chip>{TIER_LABELS.ask}</Chip> : null}
              </div>
            ))}
          </div>
        )}

        {account === null ? (
          <span className="mt-1 text-[12px] text-main-500">{NO_ACCOUNT_DATA}</span>
        ) : (
          <>
            <div className={`mt-0.5 ${HEADING}`}>{USED_BY_HEADING}</div>
            {usedBy.length === 0 ? (
              <span className="text-[12px] text-main-500">{NOT_USED}</span>
            ) : (
              <div className="flex flex-none flex-col gap-1.5">
                {usedBy.map((scenario) => (
                  <button
                    key={scenario.id}
                    type="button"
                    className="flex min-w-0 items-center gap-2.25 rounded-card border border-line bg-main-900 px-2.75 py-2 text-left hover:border-line-strong"
                    onClick={() => {
                      void navigate(`${ROUTES.scenarios}?id=${encodeURIComponent(scenario.id)}`);
                    }}
                  >
                    <Icon path={mdiSitemapOutline} size={15} className="flex-none text-main-400" />
                    <span className="flex-1 truncate text-[12px] text-main-100">
                      {scenario.name}
                    </span>
                  </button>
                ))}
              </div>
            )}

            <div className={`mt-0.5 ${HEADING}`}>{HEALTH_HEADING}</div>
            <div className="flex flex-none flex-col gap-2.25 rounded-card border border-line bg-main-900 p-3">
              {health === null ? null : (
                <div className="flex items-start gap-1.75 text-[11.5px] text-main-200">
                  <Icon path={mdiHeartPulse} size={14} className="mt-px flex-none text-main-400" />
                  <span className="min-w-0">{health}</span>
                </div>
              )}
              {metrics.map((metric) => (
                <div key={metric.label} className="flex justify-between gap-2 text-[11.5px]">
                  <span className="text-main-400">{metric.label}</span>
                  <span
                    className={`font-mono ${metric.warn === true ? "text-warn" : "text-main-100"}`}
                  >
                    {metric.value.toLocaleString("ru-RU")}
                  </span>
                </div>
              ))}
            </div>

            <div className={`mt-0.5 ${HEADING}`}>{ACTIVITY_HEADING}</div>
            <div className="flex min-h-40 flex-none flex-col rounded-card border border-line bg-main-900 px-2.75 py-2.5 font-mono text-[10.5px]/[1.8]">
              {store.activity.length === 0 ? (
                <span className="text-main-500">{NO_ACTIVITY}</span>
              ) : (
                store.activity.map((line, index) => (
                  <div
                    key={`${String(line.at)}-${String(index)}`}
                    className={LEVEL_TEXT[line.level]}
                  >
                    <span className="text-main-500">{formatLogTime(line.at)}</span>{" "}
                    <span className="wrap-break-word">{line.message}</span>
                  </div>
                ))
              )}
            </div>
          </>
        )}
      </ScrollArea>
    </aside>
  );
}

export default observer(AutomationRail);
