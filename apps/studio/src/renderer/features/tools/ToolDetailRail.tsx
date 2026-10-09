import {
  mdiAlertCircleOutline,
  mdiBookOpenPageVariantOutline,
  mdiGraphOutline,
  mdiWrenchOutline,
} from "@mdi/js";
import { observer } from "mobx-react-lite";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { useNavigate } from "react-router-dom";
import type { ToolTier } from "@zvs/shared";
import { ROUTES } from "../../app/routes";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import SegmentedControl from "../../ui/atoms/SegmentedControl";
import EmptyState from "../../ui/molecules/EmptyState";
import SchemaView from "./SchemaView";
import type ToolStore from "./ToolStore";
import { sourceLabel } from "./toolPresentation";
import TryItPanel from "./TryItPanel";

const TIER_OPTIONS: readonly { value: ToolTier; label: string }[] = [
  { value: "auto", label: "Auto" },
  { value: "ask", label: "Ask" },
  { value: "off", label: "Off" },
];

const SECTION = "text-[11px] font-semibold tracking-[0.08em] text-main-400 uppercase";

function ToolDetailRail({ store }: { readonly store: ToolStore }) {
  const navigate = useNavigate();
  const tool = store.selected;
  const tier = store.selectedTier;
  const problem = store.selectedUnavailability;
  const references = store.detail?.id === tool?.id ? (store.detail?.references ?? null) : null;

  return (
    <aside
      aria-label="Инструмент"
      className="flex min-h-0 w-85 flex-none flex-col border-l border-main-750 bg-main-900"
    >
      {tool === null || tier === null ? (
        <div className="flex flex-1 items-center justify-center">
          <EmptyState
            icon={mdiWrenchOutline}
            title="Инструмент не выбран"
            description="Выберите инструмент в таблице, чтобы увидеть его схему и попробовать вызов."
          />
        </div>
      ) : (
        <ScrollArea className="flex min-h-0 flex-1 flex-col gap-3 px-3.5 py-4">
          <div>
            <h2 className="m-0 font-mono text-[13.5px] font-medium break-all text-main-50">
              {tool.key}
            </h2>
            {tool.displayName !== tool.key ? (
              <p className="m-0 mt-0.5 text-[12px] text-main-300">{tool.displayName}</p>
            ) : null}
            <p className="m-0 mt-1 text-[11.5px] leading-normal text-main-400">
              {tool.description.length > 0 ? tool.description : "Описание не задано источником."}
            </p>
          </div>

          <div className="flex flex-wrap gap-1.25">
            <Chip tone={tool.source === "builtin" ? "accent" : "neutral"}>
              {sourceLabel(tool, store.groups)}
            </Chip>
            {tool.source === "builtin" ? null : <Chip>{tool.source}</Chip>}
            {tool.readOnly ? <Chip>read-only</Chip> : null}
            {tool.destructive ? <Chip>destructive</Chip> : null}
            {tool.enabled ? null : <Chip>выключен</Chip>}
          </div>

          {problem === null ? null : (
            <div
              role="status"
              className="flex gap-2 rounded-card border border-warn-border bg-warn-soft px-3 py-2.5 text-[12px]"
            >
              <Icon path={mdiAlertCircleOutline} size={15} className="mt-px flex-none text-warn" />
              <div className="flex min-w-0 flex-col gap-0.5">
                <span className="font-medium text-main-100">{problem.title}</span>
                {problem.detail === null ? null : (
                  <span className="wrap-break-word text-main-400">{problem.detail}</span>
                )}
                {tool.source === "mcp" ? (
                  <button
                    type="button"
                    className="mt-1 self-start text-[11.5px] font-medium text-accent-medium hover:underline"
                    onClick={() => void navigate(ROUTES.connections)}
                  >
                    Открыть «Коннекторы»
                  </button>
                ) : null}
              </div>
            </div>
          )}

          <div>
            <span className="mb-1.5 block text-[11px] font-medium text-main-400">Разрешение</span>
            <div className={store.isBusy(tool.id) ? "pointer-events-none opacity-60" : undefined}>
              <SegmentedControl
                label="Разрешение инструмента"
                value={tier}
                options={TIER_OPTIONS}
                onChange={(next) => {
                  void store.setTier(tool.id, next);
                }}
              />
            </div>
            {tool.destructive && tier !== "off" ? (
              <p className="m-0 mt-1.5 text-[11px] text-warn">
                Разрушающий инструмент включён: каждый пробный вызов потребует подтверждения.
              </p>
            ) : null}
          </div>

          <h3 className={`m-0 mt-0.5 ${SECTION}`}>Входная схема</h3>
          <SchemaView schema={tool.inputSchema} />

          <h3 className={`m-0 mt-0.5 ${SECTION}`}>Используется в</h3>
          {references === null ? (
            <span className="text-[12px] text-main-500">Загрузка…</span>
          ) : references.length === 0 ? (
            <span className="text-[12px] text-main-500">
              Ни один сценарий или навык не ссылается на инструмент.
            </span>
          ) : (
            <div className="flex flex-col gap-1.25">
              {references.map((reference) => (
                <button
                  key={`${reference.kind}:${reference.refId}`}
                  type="button"
                  className="flex items-center gap-2.25 rounded-card border border-main-750 bg-main-900 px-2.75 py-2.25 text-left hover:border-accent-line hover:bg-accent-soft"
                  onClick={() =>
                    void navigate(
                      `${reference.kind === "scenario" ? ROUTES.scenarios : ROUTES.skills}?id=${encodeURIComponent(reference.refId)}`,
                    )
                  }
                >
                  <Icon
                    path={
                      reference.kind === "scenario"
                        ? mdiGraphOutline
                        : mdiBookOpenPageVariantOutline
                    }
                    size={14}
                    className="flex-none text-main-400"
                  />
                  <span className="min-w-0 flex-1 truncate text-[12px] text-main-100">
                    {reference.name}
                  </span>
                  <Chip>{reference.kind === "scenario" ? "сценарий" : "навык"}</Chip>
                </button>
              ))}
            </div>
          )}

          <div className="min-h-2 flex-1" />

          <h3 className={`m-0 ${SECTION}`}>Попробовать</h3>
          <TryItPanel store={store} />
        </ScrollArea>
      )}
    </aside>
  );
}

export default observer(ToolDetailRail);
