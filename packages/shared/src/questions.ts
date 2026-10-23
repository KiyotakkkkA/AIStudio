import { z } from "zod";
import { ConversationId, RunId } from "./primitives/branded.js";

export const QUESTIONS_MAX = 6;
export const QUESTION_OPTIONS_MIN = 2;
export const QUESTION_OPTIONS_MAX = 8;
export const QUESTION_ANSWER_MAX = 4000;

const QuestionItemId = z.string().min(1).max(32);
const trimmedName = (max: number) =>
  z
    .string()
    .refine((value) => value.trim().length > 0)
    .refine((value) => value.trim().length <= max);

export const QuestionKind = z.enum(["text", "single", "multiple"]);
export type QuestionKind = z.infer<typeof QuestionKind>;

export const QuestionOptionDto = z.object({
  id: QuestionItemId,
  label: trimmedName(200),
  description: z.string().max(500).optional(),
  recommended: z.boolean(),
});
export type QuestionOptionDto = z.infer<typeof QuestionOptionDto>;

export const QuestionDto = z
  .object({
    id: QuestionItemId,
    title: trimmedName(300),
    kind: QuestionKind,
    options: z.array(QuestionOptionDto).max(QUESTION_OPTIONS_MAX),
    allowOther: z.boolean(),
  })
  .refine(
    (question) =>
      question.kind === "text"
        ? question.options.length === 0 && !question.allowOther
        : question.options.length >= QUESTION_OPTIONS_MIN,
    { path: ["options"] },
  )
  .refine(
    (question) =>
      new Set(question.options.map((option) => option.id)).size === question.options.length,
    { path: ["options"] },
  )
  .refine((question) => question.options.filter((option) => option.recommended).length <= 1, {
    path: ["options"],
  });
export type QuestionDto = z.infer<typeof QuestionDto>;

export const QuestionRequestDto = z
  .object({
    id: z.uuid(),
    runId: RunId,
    conversationId: ConversationId.optional(),
    questions: z.array(QuestionDto).min(1).max(QUESTIONS_MAX),
  })
  .refine(
    (request) =>
      new Set(request.questions.map((question) => question.id)).size === request.questions.length,
    { path: ["questions"] },
  );
export type QuestionRequestDto = z.infer<typeof QuestionRequestDto>;

export const QuestionAnswerDto = z.object({
  questionId: QuestionItemId,
  selected: z.array(QuestionItemId).max(QUESTION_OPTIONS_MAX),
  text: z.string().max(QUESTION_ANSWER_MAX).optional(),
});
export type QuestionAnswerDto = z.infer<typeof QuestionAnswerDto>;

export const QuestionRecordDto = z
  .object({
    request: QuestionRequestDto,
    answers: z.array(QuestionAnswerDto).max(QUESTIONS_MAX),
    skipped: z.boolean(),
  })
  .refine((record) => !record.skipped || record.answers.length === 0, { path: ["answers"] });
export type QuestionRecordDto = z.infer<typeof QuestionRecordDto>;

export const ChatAnswerInput = z.object({
  id: RunId,
  requestId: z.uuid(),
  answers: z.array(QuestionAnswerDto).min(1).max(QUESTIONS_MAX),
});
export type ChatAnswerInput = z.infer<typeof ChatAnswerInput>;

export const ChatSkipInput = z.object({
  id: RunId,
  requestId: z.uuid(),
});
export type ChatSkipInput = z.infer<typeof ChatSkipInput>;
