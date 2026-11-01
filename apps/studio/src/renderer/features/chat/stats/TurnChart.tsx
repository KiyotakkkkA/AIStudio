import { useState } from "react";
import { compactTokens } from "../ChatContextMeter";
import type { TurnRow } from "./chatStatsModel";
import {
  SERIES_CACHED,
  SERIES_INPUT,
  SERIES_OUTPUT,
  TURN_ESTIMATED,
  TURN_REPORTED,
  callsWord,
  roundsWord,
} from "./chatStatsCopy";

const CHART_HEIGHT = 160;
const TURNS_SHOWN = 40;

const SERIES = [
  { key: "input", label: SERIES_INPUT, fill: "bg-accent-dark" },
  { key: "cached", label: SERIES_CACHED, fill: "bg-accent-dark/40" },
  { key: "output", label: SERIES_OUTPUT, fill: "bg-info" },
] as const;

function seconds(durationMs: number): string {
  return `${(durationMs / 1000).toLocaleString("ru-RU", { maximumFractionDigits: 1 })} с`;
}

function Legend() {
  return (
    <ul className="flex flex-wrap gap-x-4 gap-y-1 text-[11px] text-main-400">
      {SERIES.map((series) => (
        <li key={series.key} className="flex items-center gap-1.5">
          <span className={`size-2.5 rounded-sm ${series.fill}`} />
          {series.label}
        </li>
      ))}
    </ul>
  );
}

function Tooltip({ row }: { readonly row: TurnRow }) {
  return (
    <div className="pointer-events-none absolute bottom-full left-1/2 z-10 mb-2 w-52 -translate-x-1/2 rounded-lg border border-line bg-main-900 p-2.5 text-[11.5px] text-main-300 shadow-pop">
      <p className="mb-1 font-semibold text-main-100">
        Ответ {row.index}
        <span className="ml-1.5 font-normal text-main-500">
          {row.estimated ? TURN_ESTIMATED : TURN_REPORTED}
        </span>
      </p>
      <dl className="grid grid-cols-[1fr_auto] gap-x-3 gap-y-0.5 font-mono text-[11px]">
        <dt className="font-sans">{SERIES_INPUT}</dt>
        <dd>{row.input.toLocaleString("ru-RU")}</dd>
        {row.cached > 0 ? (
          <>
            <dt className="font-sans">{SERIES_CACHED}</dt>
            <dd>{row.cached.toLocaleString("ru-RU")}</dd>
          </>
        ) : null}
        <dt className="font-sans">{SERIES_OUTPUT}</dt>
        <dd>{row.output.toLocaleString("ru-RU")}</dd>
        {row.reasoning > 0 ? (
          <>
            <dt className="font-sans">из них рассуждения</dt>
            <dd>{row.reasoning.toLocaleString("ru-RU")}</dd>
          </>
        ) : null}
      </dl>
      <p className="mt-1.5 text-main-500">
        {seconds(row.durationMs)} · {roundsWord(row.rounds)} · {callsWord(row.toolCalls)}
      </p>
    </div>
  );
}

export function TurnChart({ rows }: { readonly rows: readonly TurnRow[] }) {
  const [hovered, setHovered] = useState<string | null>(null);
  const shown = rows.slice(-TURNS_SHOWN);
  const peak = Math.max(1, ...shown.map((row) => row.input + row.output));
  const height = (value: number) => (value / peak) * CHART_HEIGHT;
  return (
    <figure className="space-y-2.5">
      <Legend />
      <div
        role="img"
        aria-label={`Расход токенов по последним ответам, максимум ${compactTokens(peak)}`}
        className="relative flex items-end gap-1 border-b border-line pt-6"
        style={{ height: CHART_HEIGHT + 24 }}
      >
        <span className="absolute top-0 left-0 font-mono text-[10.5px] text-main-500">
          {compactTokens(peak)}
        </span>
        {shown.map((row) => {
          const fresh = row.input - row.cached;
          return (
            <div
              key={row.id}
              className="relative flex h-full min-w-1.5 flex-1 flex-col justify-end"
              onMouseEnter={() => setHovered(row.id)}
              onMouseLeave={() => setHovered(null)}
              onFocus={() => setHovered(row.id)}
              onBlur={() => setHovered(null)}
              tabIndex={0}
              aria-label={`Ответ ${String(row.index)}: вход ${String(row.input)}, выход ${String(row.output)}`}
            >
              {hovered === row.id ? <Tooltip row={row} /> : null}
              <div
                className={`flex w-full flex-col justify-end gap-0.5 ${hovered !== null && hovered !== row.id ? "opacity-50" : ""}`}
              >
                {row.output > 0 ? (
                  <span
                    className="block w-full rounded-t-sm bg-info"
                    style={{ height: Math.max(2, height(row.output)) }}
                  />
                ) : null}
                {row.cached > 0 ? (
                  <span
                    className={`block w-full bg-accent-dark/40 ${row.output > 0 ? "" : "rounded-t-sm"}`}
                    style={{ height: Math.max(2, height(row.cached)) }}
                  />
                ) : null}
                {fresh > 0 ? (
                  <span
                    className={`block w-full bg-accent-dark ${row.output > 0 || row.cached > 0 ? "" : "rounded-t-sm"}`}
                    style={{ height: Math.max(2, height(fresh)) }}
                  />
                ) : null}
              </div>
            </div>
          );
        })}
      </div>
      {rows.length > TURNS_SHOWN ? (
        <figcaption className="text-[11px] text-main-500">
          Показаны последние {TURNS_SHOWN} ответов из {rows.length.toLocaleString("ru-RU")}.
        </figcaption>
      ) : null}
    </figure>
  );
}

export function TurnTable({ rows }: { readonly rows: readonly TurnRow[] }) {
  return (
    <div className="overflow-x-auto">
      <table className="w-full text-[11.5px]">
        <thead className="text-left text-main-500">
          <tr>
            <th className="py-1 pr-3 font-medium">№</th>
            <th className="py-1 pr-3 text-right font-medium">{SERIES_INPUT}</th>
            <th className="py-1 pr-3 text-right font-medium">{SERIES_CACHED}</th>
            <th className="py-1 pr-3 text-right font-medium">{SERIES_OUTPUT}</th>
            <th className="py-1 pr-3 text-right font-medium">Рассуждения</th>
            <th className="py-1 pr-3 text-right font-medium">Время</th>
            <th className="py-1 pr-3 text-right font-medium">Раунды</th>
            <th className="py-1 text-right font-medium">Вызовы</th>
          </tr>
        </thead>
        <tbody className="divide-y divide-line font-mono text-main-300">
          {rows.map((row) => (
            <tr key={row.id}>
              <td className="py-1 pr-3">
                {row.index}
                {row.estimated ? <span className="ml-1 font-sans text-main-500">*</span> : null}
              </td>
              <td className="py-1 pr-3 text-right">{row.input.toLocaleString("ru-RU")}</td>
              <td className="py-1 pr-3 text-right">{row.cached.toLocaleString("ru-RU")}</td>
              <td className="py-1 pr-3 text-right">{row.output.toLocaleString("ru-RU")}</td>
              <td className="py-1 pr-3 text-right">{row.reasoning.toLocaleString("ru-RU")}</td>
              <td className="py-1 pr-3 text-right">{seconds(row.durationMs)}</td>
              <td className="py-1 pr-3 text-right">{row.rounds}</td>
              <td className="py-1 text-right">{row.toolCalls}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <p className="mt-1.5 text-[11px] text-main-500">
        * {TURN_ESTIMATED}: провайдер не передал расход.
      </p>
    </div>
  );
}
