import type { DocumentSelectionDto, MessageId } from "@zvs/shared";
import { clipSelection, rangeOffsets } from "../../viewer/viewerModel";
import { ANSWER_QUOTE_NAME } from "./selectionCopy";

export function answerQuote(
  root: Node,
  range: Range,
  messageId: MessageId,
): DocumentSelectionDto | null {
  if (range.collapsed) return null;
  if (!root.contains(range.startContainer) || !root.contains(range.endContainer)) return null;
  const offsets = rangeOffsets(root, range);
  const text = clipSelection(range.toString());
  if (offsets === null || text.length === 0) return null;
  return {
    documentId: null,
    messageId,
    name: ANSWER_QUOTE_NAME,
    page: null,
    start: offsets.start,
    end: offsets.end,
    text,
    untrusted: true,
  };
}
