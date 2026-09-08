import { describe, expect, it } from "vitest";
import { EVENT_CHANNEL, HostEvent, RunOutcomeDto, type HostEvent as Event } from "../src/index.js";

const streamId = "0199aa11-1111-7111-8111-111111111111";
const envelope = { streamId, seq: 0, ts: 1_700_000_000_000 };

describe("HostEvent", () => {
  it("names the one multiplexed channel", () => {
    expect(EVENT_CHANNEL).toBe("zvs:events");
  });

  it("accepts every variant with its envelope", () => {
    const events: Event[] = [
      { ...envelope, type: "token", delta: "пр" },
      { ...envelope, type: "progress", done: 3, total: 10 },
      { ...envelope, type: "step", step: { node: "a" } },
      { ...envelope, type: "log", line: { level: "info", message: "hi" } },
      { ...envelope, type: "approval", request: { action: "submit" } },
      { ...envelope, type: "end", outcome: { status: "ok" } },
    ] as Event[];
    for (const event of events) expect(HostEvent.parse(event)).toEqual(event);
  });

  it("rejects an unknown type, a fractional seq and a non-uuid stream", () => {
    expect(HostEvent.safeParse({ ...envelope, type: "nope" }).success).toBe(false);
    expect(HostEvent.safeParse({ ...envelope, seq: 1.5, type: "token", delta: "" }).success).toBe(
      false,
    );
    expect(
      HostEvent.safeParse({ ...envelope, streamId: "nope", type: "token", delta: "" }).success,
    ).toBe(false);
  });

  it("keeps the terminal outcome closed to unknown statuses", () => {
    expect(RunOutcomeDto.parse({ status: "failed", message: "boom" })).toEqual({
      status: "failed",
      message: "boom",
    });
    expect(RunOutcomeDto.safeParse({ status: "weird" }).success).toBe(false);
  });
});
