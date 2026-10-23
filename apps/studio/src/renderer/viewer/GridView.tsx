import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { observer } from "mobx-react-lite";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import type ViewerStore from "../stores/ViewerStore";
import { columnCount, columnName, columnWidths, isNumeric, rowMatches } from "./csv";
import { EMPTY_DOCUMENT } from "./viewerCopy";
import { PAGE_ATTRIBUTE, useSelectionCapture } from "./useDocumentText";
import { locatePassage } from "./viewerModel";

export const ROW_HEIGHT = 28;
export const ROW_HEADER_WIDTH = 52;
const OVERSCAN = 12;
const COLUMNS_SHOWN = 200;

function GridView({
  viewer,
  rows,
  source = null,
  page = null,
}: {
  readonly viewer: ViewerStore;
  readonly rows: readonly (readonly string[])[];
  readonly source?: string | null;
  readonly page?: number | null;
}) {
  const scrollerRef = useRef<HTMLDivElement>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  const [frame, setFrame] = useState({ top: 0, height: 600 });
  const columns = Math.min(COLUMNS_SHOWN, columnCount(rows));
  const widths = useMemo(() => columnWidths(rows, columns), [rows, columns]);
  const matches = useMemo(() => rowMatches(rows, viewer.query), [rows, viewer.query]);
  const matched = useMemo(() => new Set(matches), [matches]);
  const current = matches[viewer.matchIndex];
  const passageRow = useMemo(() => {
    const passage = viewer.passage?.toLocaleLowerCase("ru-RU") ?? null;
    if (passage === null) return -1;
    return rows.findIndex((row) => {
      const cells = row
        .map((cell) => cell.trim().toLocaleLowerCase("ru-RU"))
        .filter((cell) => cell.length >= 3)
        .slice(0, 3);
      return cells.length > 0 && cells.every((cell) => passage.includes(cell));
    });
  }, [rows, viewer.passage]);

  useEffect(() => {
    viewer.setMatchCount(matches.length);
  }, [viewer, matches.length]);

  useLayoutEffect(() => {
    const scroller = scrollerRef.current;
    if (scroller === null) return;
    const measure = () => setFrame({ top: scroller.scrollTop, height: scroller.clientHeight });
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(scroller);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    const target = current ?? (passageRow >= 0 ? passageRow : undefined);
    const scroller = scrollerRef.current;
    if (target === undefined || scroller === null) return;
    scroller.scrollTo({ top: Math.max(0, target * ROW_HEIGHT - scroller.clientHeight / 3) });
  }, [current, passageRow]);

  const locate = useMemo(
    () => (source === null ? undefined : (text: string) => locatePassage(source, text)),
    [source],
  );
  useSelectionCapture(viewer, rootRef, locate);

  if (rows.length === 0)
    return <p className="m-auto px-6 text-center text-[13px] text-main-400">{EMPTY_DOCUMENT}</p>;

  const first = Math.max(0, Math.floor(frame.top / ROW_HEIGHT) - OVERSCAN);
  const last = Math.min(rows.length, Math.ceil((frame.top + frame.height) / ROW_HEIGHT) + OVERSCAN);
  const template = `${String(ROW_HEADER_WIDTH)}px ${widths.map((width) => `${String(width)}px`).join(" ")}`;
  const total = ROW_HEADER_WIDTH + widths.reduce((sum, width) => sum + width, 0);
  const line = "border-r border-b border-line";

  return (
    <ScrollArea
      ref={scrollerRef}
      orientation="both"
      className="min-h-0 flex-1 bg-main-900"
      onScroll={(event) => {
        const top = event.currentTarget.scrollTop;
        setFrame((value) => ({ ...value, top }));
      }}
    >
      <div
        ref={rootRef}
        {...(page === null ? {} : { [PAGE_ATTRIBUTE]: page })}
        role="table"
        className="relative text-[12.5px] text-main-100"
        style={{
          width: `${String(total)}px`,
          height: `${String((rows.length + 1) * ROW_HEIGHT)}px`,
        }}
      >
        <div
          role="row"
          className="sticky top-0 z-20 grid bg-main-850 text-[11.5px] font-medium text-main-400 select-none"
          style={{ gridTemplateColumns: template, height: `${String(ROW_HEIGHT)}px` }}
        >
          <span role="columnheader" className={`sticky left-0 z-10 bg-main-850 ${line}`} />
          {widths.map((_, column) => (
            <span
              key={column}
              role="columnheader"
              className={`flex items-center justify-center ${line}`}
            >
              {columnName(column)}
            </span>
          ))}
        </div>
        {rows.slice(first, last).map((row, offset) => {
          const index = first + offset;
          const tone =
            index === current
              ? "bg-accent-soft"
              : matched.has(index)
                ? "bg-warn-soft"
                : index === passageRow
                  ? "bg-accent-soft"
                  : "";
          return (
            <div
              key={index}
              role="row"
              className={`absolute left-0 grid ${tone}`}
              style={{
                top: `${String((index + 1) * ROW_HEIGHT)}px`,
                height: `${String(ROW_HEIGHT)}px`,
                width: `${String(total)}px`,
                gridTemplateColumns: template,
              }}
            >
              <span
                role="rowheader"
                className={`sticky left-0 z-10 flex items-center justify-center bg-main-850 text-[11.5px] text-main-500 tabular-nums select-none ${line}`}
              >
                {(index + 1).toLocaleString("ru-RU")}
              </span>
              {widths.map((_, column) => {
                const cell = row[column] ?? "";
                return (
                  <span
                    key={column}
                    role="cell"
                    title={cell.length > 0 ? cell : undefined}
                    className={`min-w-0 overflow-hidden px-2 leading-[27px] text-ellipsis whitespace-nowrap ${line} ${isNumeric(cell) ? "text-right tabular-nums" : ""}`}
                  >
                    {cell}
                  </span>
                );
              })}
            </div>
          );
        })}
      </div>
    </ScrollArea>
  );
}

export default observer(GridView);
