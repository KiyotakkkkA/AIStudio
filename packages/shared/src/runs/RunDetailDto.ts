import { z } from "zod";
import { HostEvent, LogLineDto } from "../events/HostEvent.js";
import { RunDto } from "./RunDto.js";
import { RunSummaryDto } from "./RunSummaryDto.js";
import { StepDto } from "./StepDto.js";

export const RunDetailDto = z.object({
  run: RunDto,
  summary: RunSummaryDto,
  steps: z.array(StepDto),
  logs: z.array(LogLineDto),
  events: z.array(HostEvent),
});
export type RunDetailDto = z.infer<typeof RunDetailDto>;
