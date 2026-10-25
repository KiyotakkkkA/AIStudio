import { useState, type ReactNode } from "react";
import { mdiCheckBold } from "@mdi/js";
import {
  QUESTION_ANSWER_MAX,
  type QuestionAnswerDto,
  type QuestionDto,
  type QuestionRecordDto,
  type QuestionRequestDto,
} from "@zvs/shared";
import Button from "../../ui/atoms/buttons/Button";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import TextArea from "../../ui/atoms/TextArea";
import TextInput from "../../ui/atoms/TextInput";
import {
  EMPTY_DRAFT,
  MULTIPLE_HINT,
  OTHER_OPTION,
  OTHER_PLACEHOLDER,
  QUESTIONS_LEAD,
  RECOMMENDED,
  SKIP_QUESTIONS,
  SUBMIT_ANSWERS,
  TEXT_PLACEHOLDER,
  answerText,
  draftAnswers,
  pick,
  recommendedFirst,
  toggleOther,
  writeOther,
  type QuestionDraft,
} from "./questionCopy";

function Choice({
  single,
  checked,
  disabled,
  onToggle,
  children,
}: {
  readonly single: boolean;
  readonly checked: boolean;
  readonly disabled: boolean;
  readonly onToggle: () => void;
  readonly children: ReactNode;
}) {
  return (
    <button
      type="button"
      role={single ? "radio" : "checkbox"}
      aria-checked={checked}
      disabled={disabled}
      onClick={onToggle}
      className={`flex w-full items-start gap-2.5 rounded-control px-2.5 py-1.5 text-left transition-colors disabled:cursor-not-allowed ${
        checked ? "bg-main-700" : "hover:bg-main-800"
      }`}
    >
      <span
        aria-hidden="true"
        className={`mt-0.5 flex size-3.5 shrink-0 items-center justify-center border ${
          single ? "rounded-full" : "rounded-sm"
        } ${checked ? "border-accent-dark bg-accent-dark text-on-accent" : "border-line-strong"}`}
      >
        {checked &&
          (single ? (
            <span className="size-1.5 rounded-full bg-on-accent" />
          ) : (
            <Icon path={mdiCheckBold} size={10} />
          ))}
      </span>
      <span className="min-w-0 flex-1">{children}</span>
    </button>
  );
}

function QuestionField({
  question,
  index,
  draft,
  disabled,
  onChange,
}: {
  readonly question: QuestionDto;
  readonly index: number;
  readonly draft: QuestionDraft;
  readonly disabled: boolean;
  readonly onChange: (draft: QuestionDraft) => void;
}) {
  const title = `${String(index + 1)}. ${question.title}`;
  if (question.kind === "text")
    return (
      <div className="space-y-1.5">
        <p className="m-0 text-[12.5px] font-medium text-main-100">{title}</p>
        <TextArea
          aria-label={question.title}
          placeholder={TEXT_PLACEHOLDER}
          value={draft.other[question.id] ?? ""}
          maxLength={QUESTION_ANSWER_MAX}
          disabled={disabled}
          onChange={(event) => onChange(writeOther(draft, question, event.target.value))}
        />
      </div>
    );
  const single = question.kind === "single";
  const selected = draft.selected[question.id] ?? [];
  const otherOn = draft.otherOn[question.id] ?? false;
  return (
    <div className="space-y-1.5">
      <div>
        <p className="m-0 text-[12.5px] font-medium text-main-100">{title}</p>
        {!single && <p className="m-0 text-[11px] text-main-500">{MULTIPLE_HINT}</p>}
      </div>
      <div
        role={single ? "radiogroup" : "group"}
        aria-label={question.title}
        className="flex flex-col gap-0.5"
      >
        {recommendedFirst(question).map((option) => (
          <Choice
            key={option.id}
            single={single}
            checked={selected.includes(option.id)}
            disabled={disabled}
            onToggle={() => onChange(pick(draft, question, option.id))}
          >
            <span className="flex flex-wrap items-center gap-1.5">
              <span className="text-[12.5px] text-main-100">{option.label}</span>
              {option.recommended && <Chip tone="accent">{RECOMMENDED}</Chip>}
            </span>
            {option.description !== undefined && (
              <span className="block text-[11.5px] text-main-500">{option.description}</span>
            )}
          </Choice>
        ))}
        {question.allowOther && (
          <Choice
            single={single}
            checked={otherOn}
            disabled={disabled}
            onToggle={() => onChange(toggleOther(draft, question))}
          >
            <span className="text-[12.5px] text-main-100">{OTHER_OPTION}</span>
          </Choice>
        )}
        {question.allowOther && otherOn && (
          <div className="pt-1 pl-8">
            <TextInput
              aria-label={`${question.title}: ${OTHER_PLACEHOLDER}`}
              placeholder={OTHER_PLACEHOLDER}
              value={draft.other[question.id] ?? ""}
              maxLength={QUESTION_ANSWER_MAX}
              disabled={disabled}
              autoFocus
              onChange={(event) => onChange(writeOther(draft, question, event.target.value))}
            />
          </div>
        )}
      </div>
    </div>
  );
}

export default function ChatQuestionCard({
  request,
  busy,
  onAnswer,
  onSkip,
}: {
  readonly request: QuestionRequestDto;
  readonly busy: boolean;
  readonly onAnswer: (answers: QuestionAnswerDto[]) => void;
  readonly onSkip: () => void;
}) {
  const [draft, setDraft] = useState<QuestionDraft>(EMPTY_DRAFT);
  const answers = draftAnswers(request, draft);
  return (
    <div role="group" className="space-y-3 rounded-control bg-accent-soft px-3 py-2.5">
      <p className="m-0 text-[12px] text-main-300">{QUESTIONS_LEAD}</p>
      {request.questions.map((question, index) => (
        <QuestionField
          key={question.id}
          question={question}
          index={index}
          draft={draft}
          disabled={busy}
          onChange={setDraft}
        />
      ))}
      <div className="flex gap-2">
        <Button
          size="sm"
          tone="primary"
          disabled={busy || answers.length === 0}
          onClick={() => onAnswer(answers)}
        >
          {SUBMIT_ANSWERS}
        </Button>
        <Button size="sm" disabled={busy} onClick={onSkip}>
          {SKIP_QUESTIONS}
        </Button>
      </div>
    </div>
  );
}

export function QuestionAnswers({
  record,
  note,
}: {
  readonly record: QuestionRecordDto;
  readonly note: string | null;
}) {
  return (
    <div className="space-y-2">
      <dl className="m-0 space-y-1.5">
        {record.request.questions.map((question, index) => (
          <div key={question.id} className="min-w-0">
            <dt className="text-[11.5px] text-main-500">
              {String(index + 1)}. {question.title}
            </dt>
            <dd className="m-0 text-[12.5px] wrap-break-word text-main-100">
              {answerText(
                question,
                record.answers.find((answer) => answer.questionId === question.id),
              )}
            </dd>
          </div>
        ))}
      </dl>
      {note !== null && <p className="m-0 text-[11.5px] text-main-500">{note}</p>}
    </div>
  );
}
