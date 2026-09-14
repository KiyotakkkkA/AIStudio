import { z } from "zod";
import { LogLineDto } from "../events/HostEvent.js";
import { RunDto } from "./RunDto.js";
import { RunSummaryDto } from "./RunSummaryDto.js";
import { StepDto } from "./StepDto.js";

export const RunDetailDto = z.object({
  run: RunDto,
  summary: RunSummaryDto,
  steps: z.array(StepDto),
  logs: z.array(LogLineDto),
});
export type RunDetailDto = z.infer<typeof RunDetailDto>;
