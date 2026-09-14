import { StepDto, type StepEventDto } from "@zvs/shared";

export function parseStepEvent(raw: StepEventDto): StepDto | null {
  const normalized = Object.fromEntries(
    Object.entries(raw).filter(([key, value]) => value !== null || key === "input"),
  );
  const parsed = StepDto.safeParse(normalized);
  return parsed.success ? parsed.data : null;
}
