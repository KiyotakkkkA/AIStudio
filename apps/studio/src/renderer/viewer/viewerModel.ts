export const ZOOM_PRESETS = [0.5, 0.75, 1, 1.25, 1.5, 2, 3] as const;
export const ZOOM_MIN = 0.25;
export const ZOOM_MAX = 4;
export const PAGE_GAP = 16;
export const PAGE_PADDING = 20;
export const PDF_TO_CSS = 96 / 72;

export const VIEWER_DOCK_MIN_WIDTH = 360;
export const VIEWER_DOCK_DEFAULT_WIDTH = 560;
export const THREAD_MIN_WIDTH = 420;

const RICH_EXTENSIONS = new Set([
  "pdf",
  "png",
  "jpg",
  "jpeg",
  "jfif",
  "gif",
  "webp",
  "bmp",
  "docx",
  "xlsx",
  "pptx",
  "csv",
  "tsv",
]);

const VIEWABLE_EXTENSIONS = new Set([
  ...RICH_EXTENSIONS,
  "md",
  "markdown",
  "mdx",
  "txt",
  "text",
  "log",
  "html",
  "htm",
  "svg",
  "xml",
  "json",
]);

export function extensionOf(name: string): string {
  const base = name.split(/[\\/]/u).at(-1) ?? name;
  const dot = base.lastIndexOf(".");
  return dot <= 0 ? "" : base.slice(dot + 1).toLowerCase();
}

export function opensInViewer(name: string): boolean {
  return RICH_EXTENSIONS.has(extensionOf(name));
}

export function viewerCanShow(format: string): boolean {
  return VIEWABLE_EXTENSIONS.has(format.toLowerCase());
}

export function joinFolderPath(folder: string, relative: string): string {
  const separator = folder.includes("\\") ? "\\" : "/";
  const trimmed = folder.replace(/[\\/]+$/u, "");
  const tail = relative
    .split("/")
    .filter((part) => part.length > 0)
    .join(separator);
  return `${trimmed}${separator}${tail}`;
}

export function clampZoom(zoom: number): number {
  if (!Number.isFinite(zoom)) return 1;
  return Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, Math.round(zoom * 100) / 100));
}

export function nextZoom(zoom: number, direction: 1 | -1): number {
  if (direction === 1) return ZOOM_PRESETS.find((preset) => preset > zoom + 0.001) ?? ZOOM_MAX;
  return [...ZOOM_PRESETS].reverse().find((preset) => preset < zoom - 0.001) ?? ZOOM_MIN;
}

export function fitWidthZoom(containerWidth: number, pageWidth: number): number {
  if (pageWidth <= 0 || containerWidth <= 0) return 1;
  return clampZoom((containerWidth - PAGE_PADDING * 2) / (pageWidth * PDF_TO_CSS));
}

export function clampPage(page: number, count: number): number {
  if (count <= 0) return 1;
  return Math.min(count, Math.max(1, Math.round(page)));
}

export interface PageSize {
  readonly width: number;
  readonly height: number;
}

export interface PageSlot {
  readonly page: number;
  readonly top: number;
  readonly width: number;
  readonly height: number;
}

export interface PageLayout {
  readonly slots: readonly PageSlot[];
  readonly height: number;
  readonly width: number;
}

export function pageLayout(sizes: readonly PageSize[], scale: number): PageLayout {
  let top = PAGE_PADDING;
  let width = 0;
  const slots = sizes.map((size, index) => {
    const slot = {
      page: index + 1,
      top,
      width: Math.floor(size.width * scale),
      height: Math.floor(size.height * scale),
    };
    top += slot.height + PAGE_GAP;
    width = Math.max(width, slot.width);
    return slot;
  });
  return {
    slots,
    height: slots.length === 0 ? 0 : top - PAGE_GAP + PAGE_PADDING,
    width: width + PAGE_PADDING * 2,
  };
}

export function visiblePages(
  layout: PageLayout,
  scrollTop: number,
  viewportHeight: number,
  overscan = 1,
): number[] {
  const bottom = scrollTop + viewportHeight;
  const visible = layout.slots.filter(
    (slot) => slot.top + slot.height >= scrollTop && slot.top <= bottom,
  );
  if (visible.length === 0) return [];
  const first = Math.max(1, visible[0]!.page - overscan);
  const last = Math.min(layout.slots.length, visible.at(-1)!.page + overscan);
  return Array.from({ length: last - first + 1 }, (_, index) => first + index);
}

export function pageAt(layout: PageLayout, scrollTop: number, viewportHeight: number): number {
  const probe = scrollTop + Math.min(viewportHeight / 3, 200);
  let current = 1;
  for (const slot of layout.slots) {
    if (slot.top <= probe) current = slot.page;
    else break;
  }
  return current;
}

export interface TextRange {
  readonly start: number;
  readonly end: number;
}

export function findMatches(text: string, query: string, limit = 1000): TextRange[] {
  const needle = query.trim().toLocaleLowerCase("ru-RU");
  if (needle.length === 0) return [];
  const haystack = text.toLocaleLowerCase("ru-RU");
  const found: TextRange[] = [];
  let from = 0;
  while (found.length < limit) {
    const index = haystack.indexOf(needle, from);
    if (index === -1) break;
    found.push({ start: index, end: index + needle.length });
    from = index + needle.length;
  }
  return found;
}

function normalized(text: string): { readonly folded: string; readonly origin: number[] } {
  let folded = "";
  const origin: number[] = [];
  let space = false;
  for (let index = 0; index < text.length; index += 1) {
    const char = text[index]!;
    if (/\s/u.test(char) || char === "­") {
      if (!space && folded.length > 0) {
        folded += " ";
        origin.push(index);
      }
      space = true;
      continue;
    }
    space = false;
    folded += char.toLocaleLowerCase("ru-RU");
    origin.push(index);
  }
  if (folded.endsWith(" ")) {
    folded = folded.slice(0, -1);
    origin.pop();
  }
  return { folded, origin };
}

export const PASSAGE_PROBE = 80;

export function locatePassage(text: string, passage: string): TextRange | null {
  const target = normalized(passage).folded;
  if (target.length === 0) return null;
  const source = normalized(text);
  const probe = target.slice(0, PASSAGE_PROBE);
  const at = source.folded.indexOf(probe);
  if (at === -1) return null;
  let length = probe.length;
  while (
    at + length < source.folded.length &&
    length < target.length &&
    source.folded[at + length] === target[length]
  ) {
    length += 1;
  }
  const start = source.origin[at]!;
  const end = source.origin[at + length - 1]! + 1;
  return { start, end };
}

export function textNodes(root: Node): Text[] {
  const nodes: Text[] = [];
  const walker = root.ownerDocument!.createTreeWalker(root, 4);
  for (let node = walker.nextNode(); node !== null; node = walker.nextNode()) {
    nodes.push(node as Text);
  }
  return nodes;
}

export function offsetWithin(root: Node, container: Node, offset: number): number | null {
  if (!root.contains(container)) return null;
  let total = 0;
  if (container.nodeType !== 3) {
    const range = root.ownerDocument!.createRange();
    range.selectNodeContents(root);
    range.setEnd(container, offset);
    return range.toString().length;
  }
  for (const node of textNodes(root)) {
    if (node === container) return total + offset;
    total += node.data.length;
  }
  return null;
}

export function rangeOffsets(root: Node, range: Range): TextRange | null {
  const length = textNodes(root).reduce((sum, node) => sum + node.data.length, 0);
  const startInside = root.contains(range.startContainer);
  const endInside = root.contains(range.endContainer);
  if (!startInside && !endInside) return null;
  const start = startInside ? offsetWithin(root, range.startContainer, range.startOffset) : 0;
  const end = endInside ? offsetWithin(root, range.endContainer, range.endOffset) : length;
  if (start === null || end === null || end <= start) return null;
  return { start, end };
}

export function rangeAt(root: Node, start: number, end: number): Range | null {
  if (end <= start) return null;
  const range = root.ownerDocument!.createRange();
  let total = 0;
  let started = false;
  for (const node of textNodes(root)) {
    const next = total + node.data.length;
    if (!started && start < next) {
      range.setStart(node, start - total);
      started = true;
    }
    if (started && end <= next) {
      range.setEnd(node, end - total);
      return range;
    }
    total = next;
  }
  return null;
}

export const SELECTION_TEXT_LIMIT = 8000;

export function clipSelection(text: string): string {
  const trimmed = text.replace(/\s+$/u, "").replace(/^\s+/u, "");
  return trimmed.length <= SELECTION_TEXT_LIMIT ? trimmed : trimmed.slice(0, SELECTION_TEXT_LIMIT);
}

export function decodeBase64(data: string): Uint8Array {
  const binary = atob(data);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) bytes[index] = binary.charCodeAt(index);
  return bytes;
}

export function joinBytes(parts: readonly Uint8Array[], total: number): Uint8Array {
  const bytes = new Uint8Array(total);
  let offset = 0;
  for (const part of parts) {
    bytes.set(part.subarray(0, Math.max(0, total - offset)), offset);
    offset += part.length;
  }
  return bytes;
}
