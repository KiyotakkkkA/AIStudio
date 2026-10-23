import { useEffect, useRef, type RefObject } from "react";
import type ViewerStore from "../stores/ViewerStore";
import {
  CURRENT_HIGHLIGHT,
  MATCH_HIGHLIGHT,
  PASSAGE_HIGHLIGHT,
  clearPaint,
  paint,
} from "./highlights";
import { findMatches, locatePassage, rangeAt, rangeOffsets, textNodes } from "./viewerModel";

export const PAGE_ATTRIBUTE = "data-viewer-page";

export function pageElement(node: Node, root: Node): HTMLElement | null {
  const element = node.nodeType === 1 ? (node as Element) : node.parentElement;
  const page = element?.closest<HTMLElement>(`[${PAGE_ATTRIBUTE}]`) ?? null;
  return page !== null && root.contains(page) ? page : null;
}

export function pageNumber(element: HTMLElement | null): number | null {
  const value = Number(element?.getAttribute(PAGE_ATTRIBUTE));
  return Number.isInteger(value) && value > 0 ? value : null;
}

export function scrollRangeIntoView(scroller: HTMLElement, range: Range): void {
  const rect = range.getBoundingClientRect();
  const frame = scroller.getBoundingClientRect();
  if (rect.height === 0 && rect.width === 0) return;
  const offset = rect.top - frame.top - frame.height / 3;
  scroller.scrollTo({ top: scroller.scrollTop + offset, behavior: "smooth" });
  const horizontal = rect.left - frame.left;
  if (horizontal < 0 || horizontal > frame.width - 40)
    scroller.scrollTo({ left: scroller.scrollLeft + horizontal - 40 });
}

export function useSelectionCapture(
  viewer: ViewerStore,
  root: RefObject<HTMLElement | null>,
  locate?: (text: string) => { start: number; end: number } | null,
): void {
  useEffect(() => {
    let frame = 0;
    const capture = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        const element = root.current;
        const selection = document.getSelection();
        if (element === null || selection === null || selection.rangeCount === 0) return;
        const range = selection.getRangeAt(0);
        const inside =
          element.contains(range.startContainer) || element.contains(range.endContainer);
        if (!inside) return;
        if (selection.isCollapsed) {
          viewer.select(null);
          return;
        }
        const text = selection.toString();
        const page = pageElement(range.startContainer, element);
        const scope = page ?? element;
        const offsets = locate?.(text) ?? rangeOffsets(scope, range);
        if (offsets === null) return;
        viewer.select({ page: pageNumber(page), start: offsets.start, end: offsets.end, text });
      });
    };
    document.addEventListener("selectionchange", capture);
    return () => {
      cancelAnimationFrame(frame);
      document.removeEventListener("selectionchange", capture);
    };
  }, [viewer, root, locate]);
}

export function useDocumentText(
  viewer: ViewerStore,
  root: RefObject<HTMLElement | null>,
  scroller: RefObject<HTMLElement | null>,
  version: number,
): void {
  const passageShown = useRef<string | null>(null);
  const matchShown = useRef<string | null>(null);
  const query = viewer.query;
  const matchIndex = viewer.matchIndex;
  const passage = viewer.passage;

  useSelectionCapture(viewer, root);

  useEffect(() => () => clearPaint(), []);

  useEffect(() => {
    const element = root.current;
    if (element === null) return;
    const full = textNodes(element)
      .map((node) => node.data)
      .join("");
    const matches = findMatches(full, query);
    viewer.setMatchCount(matches.length);
    const ranges = matches
      .map((match) => rangeAt(element, match.start, match.end))
      .filter((range): range is Range => range !== null);
    paint(MATCH_HIGHLIGHT, ranges);
    const current = ranges[Math.min(matchIndex, ranges.length - 1)];
    paint(CURRENT_HIGHLIGHT, current === undefined ? [] : [current]);
    const key = `${query}:${String(matchIndex)}`;
    if (current !== undefined && scroller.current !== null && matchShown.current !== key) {
      matchShown.current = key;
      scrollRangeIntoView(scroller.current, current);
    }
  }, [viewer, root, scroller, query, matchIndex, version]);

  useEffect(() => {
    const element = root.current;
    if (element === null || passage === null) {
      paint(PASSAGE_HIGHLIGHT, []);
      return;
    }
    const full = textNodes(element)
      .map((node) => node.data)
      .join("");
    const located = locatePassage(full, passage);
    const range = located === null ? null : rangeAt(element, located.start, located.end);
    paint(PASSAGE_HIGHLIGHT, range === null ? [] : [range]);
    if (range !== null && scroller.current !== null && passageShown.current !== passage) {
      passageShown.current = passage;
      scrollRangeIntoView(scroller.current, range);
    }
  }, [root, scroller, passage, version]);
}
