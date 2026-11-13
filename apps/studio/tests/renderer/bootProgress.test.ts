import { describe, expect, it } from "vitest";

import { describeProgress, formatSize } from "../../src/boot/bootProgress";

describe("boot progress", () => {
  it("formats sizes with the Russian locale and one decimal above bytes", () => {
    expect(formatSize(0)).toBe("0 Б");
    expect(formatSize(512)).toBe("512 Б");
    expect(formatSize(1536)).toBe("1,5 КБ");
    expect(formatSize(4.3 * 1024 ** 3)).toBe("4,3 ГБ");
    expect(formatSize(Number.NaN)).toBe("0 Б");
  });

  it("reports a floored percentage and the amount moved", () => {
    expect(
      describeProgress({
        step: 1,
        steps: 1,
        label: "Перенос",
        done: 1024 ** 3,
        total: 3 * 1024 ** 3,
      }),
    ).toEqual({ label: "Перенос", percent: 33, amount: "1,0 ГБ из 3,0 ГБ" });
  });

  it("names the step only when there are several and treats an empty step as complete", () => {
    expect(describeProgress({ step: 2, steps: 3, label: "Перенос", done: 0, total: 0 })).toEqual({
      label: "Перенос (шаг 2 из 3)",
      percent: 100,
      amount: "",
    });
    expect(
      describeProgress({ step: 1, steps: 1, label: "Перенос", done: 9, total: 4 }).percent,
    ).toBe(100);
  });
});
