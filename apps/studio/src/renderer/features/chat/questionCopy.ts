import {
  QuestionRecordDto,
  type Json,
  type QuestionAnswerDto,
  type QuestionDto,
  type QuestionRequestDto,
} from "@zvs/shared";
import { plural } from "../connections/connectionPresentation";

export const ASK_TOOL = "chat.ask";

export const QUESTIONS_TITLE = "Уточняющие вопросы";
export const QUESTIONS_LEAD = "Ответ продолжит выполнение запроса.";
export const RECOMMENDED = "Рекомендуется";
export const OTHER_OPTION = "Другое";
export const OTHER_PLACEHOLDER = "Свой вариант";
export const TEXT_PLACEHOLDER = "Ответ";
export const MULTIPLE_HINT = "Можно выбрать несколько вариантов";
export const SUBMIT_ANSWERS = "Ответить";
export const SKIP_QUESTIONS = "Пропустить";
export const QUESTIONS_SKIPPED = "Вопросы пропущены";
export const QUESTIONS_UNANSWERED = "Вопрос остался без ответа";
export const NO_ANSWER = "Без ответа";
export const ANSWER_FAILED = "Не удалось отправить ответ. Попробуйте ещё раз.";

export function questionsWord(count: number): string {
  return plural(count, "вопрос", "вопроса", "вопросов");
}

export function questionRecord(meta: Json): QuestionRecordDto | null {
  const parsed = QuestionRecordDto.safeParse(meta);
  return parsed.success ? parsed.data : null;
}

export function recommendedFirst(question: QuestionDto): QuestionDto["options"] {
  return [...question.options].sort(
    (left, right) => Number(right.recommended) - Number(left.recommended),
  );
}

export function answerText(question: QuestionDto, answer: QuestionAnswerDto | undefined): string {
  if (answer === undefined) return NO_ANSWER;
  const labels = answer.selected.flatMap((id) => {
    const option = question.options.find((item) => item.id === id);
    return option === undefined ? [] : [option.label];
  });
  if (answer.text !== undefined)
    labels.push(question.kind === "text" ? answer.text : `${OTHER_OPTION}: ${answer.text}`);
  return labels.length === 0 ? NO_ANSWER : labels.join(", ");
}

export function recordSummary(record: QuestionRecordDto, pending: boolean): string {
  const count = questionsWord(record.request.questions.length);
  if (record.skipped) return `${count} · ${QUESTIONS_SKIPPED.toLowerCase()}`;
  const [only] = record.request.questions;
  if (record.answers.length > 0)
    return record.request.questions.length === 1 && only !== undefined
      ? answerText(only, record.answers[0])
      : `${count} · ответов: ${record.answers.length.toLocaleString("ru-RU")}`;
  return pending ? count : `${count} · без ответа`;
}

export interface QuestionDraft {
  readonly selected: Readonly<Record<string, readonly string[]>>;
  readonly other: Readonly<Record<string, string>>;
  readonly otherOn: Readonly<Record<string, boolean>>;
}

export const EMPTY_DRAFT: QuestionDraft = { selected: {}, other: {}, otherOn: {} };

export function pick(draft: QuestionDraft, question: QuestionDto, optionId: string): QuestionDraft {
  const current = draft.selected[question.id] ?? [];
  if (question.kind === "single")
    return {
      ...draft,
      selected: { ...draft.selected, [question.id]: [optionId] },
      otherOn: { ...draft.otherOn, [question.id]: false },
    };
  return {
    ...draft,
    selected: {
      ...draft.selected,
      [question.id]: current.includes(optionId)
        ? current.filter((id) => id !== optionId)
        : [...current, optionId],
    },
  };
}

export function toggleOther(draft: QuestionDraft, question: QuestionDto): QuestionDraft {
  const on = !(draft.otherOn[question.id] ?? false);
  return {
    ...draft,
    otherOn: { ...draft.otherOn, [question.id]: on },
    selected:
      question.kind === "single" && on ? { ...draft.selected, [question.id]: [] } : draft.selected,
  };
}

export function writeOther(
  draft: QuestionDraft,
  question: QuestionDto,
  text: string,
): QuestionDraft {
  return { ...draft, other: { ...draft.other, [question.id]: text } };
}

export function draftAnswers(
  request: QuestionRequestDto,
  draft: QuestionDraft,
): QuestionAnswerDto[] {
  return request.questions.flatMap((question) => {
    const selected = question.kind === "text" ? [] : [...(draft.selected[question.id] ?? [])];
    const typed = (draft.other[question.id] ?? "").trim();
    const textOn = question.kind === "text" || (draft.otherOn[question.id] ?? false);
    const text = textOn && typed.length > 0 ? typed : undefined;
    if (selected.length === 0 && text === undefined) return [];
    return [{ questionId: question.id, selected, ...(text === undefined ? {} : { text }) }];
  });
}
