import type { Json, StepDto } from "@zvs/shared";

export interface KnowledgeStepPassage {
  readonly path: string;
  readonly section: string;
  readonly score: number;
  readonly text: string;
}

export interface KnowledgeStep {
  readonly query: string;
  readonly passages: readonly KnowledgeStepPassage[];
}

function record(value: Json | undefined): Record<string, Json> | null {
  return typeof value === "object" && value !== null && !Array.isArray(value) ? value : null;
}

export function knowledgeSearchStep(step: StepDto): KnowledgeStep | null {
  if (step.type !== "knowledge.search") return null;
  const input = record(step.input);
  const query = input?.query;
  if (typeof query !== "string") return null;
  const listed = record(step.output)?.passages;
  const passages = Array.isArray(listed)
    ? listed.flatMap((entry) => {
        const passage = record(entry);
        if (passage === null || typeof passage.path !== "string") return [];
        return [
          {
            path: passage.path,
            section: typeof passage.section === "string" ? passage.section : "",
            score: typeof passage.score === "number" ? passage.score : 0,
            text: typeof passage.text === "string" ? passage.text : "",
          },
        ];
      })
    : [];
  return { query, passages };
}
