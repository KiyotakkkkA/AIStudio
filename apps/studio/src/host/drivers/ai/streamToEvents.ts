import { AppErrorCode, isAppError, type RunOutcomeDto } from "@zvs/shared";
import type { StreamHandle } from "../../platform/events.ts";
import { isCancellation, toAppError } from "./errors.ts";
import type { TextDelta } from "./ports.ts";

export interface StreamToEventsResult {
  readonly text: string;
  readonly outcome: RunOutcomeDto;
}

export async function streamToEvents(
  stream: StreamHandle,
  deltas: AsyncIterable<TextDelta>,
): Promise<StreamToEventsResult> {
  let text = "";
  let outcome: RunOutcomeDto = { status: "ok" };
  try {
    for await (const delta of deltas) {
      if (delta.text.length === 0) continue;
      text += delta.text;
      stream.emit({ type: "token", delta: delta.text });
    }
  } catch (error: unknown) {
    outcome = toOutcome(error);
  }
  stream.end(outcome);
  return { text, outcome };
}

function toOutcome(error: unknown): RunOutcomeDto {
  if (isCancellation(error)) {
    return { status: "cancelled", code: AppErrorCode.RUN_CANCELLED, message: "Отменено" };
  }
  const failure = isAppError(error) ? error : toAppError(error);
  return { status: "failed", code: failure.code, message: failure.message };
}
