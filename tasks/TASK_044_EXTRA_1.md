---
task: TASK_044_EXTRA_1
title: Questions to the user — the chat.ask bridge, a question gate in the kernel, the answer card
phase: 8 — Documents and the agent browser
status: done
depends_on: [TASK_044, TASK_036_EXTRA_3]
unblocks: [TASK_040]
estimate: 4 h
---

# TASK_044_EXTRA_1 — Questions to the user

## Context

A chat turn is one run: `chat.prepare` → `chat.generate` → `chat.persist`. Inside
`chat.generate` the tool-calling loop (`services/chat.rs`, `respond`) offers the model the tools
picked for the dialog, runs the calls and feeds the results back. When a tool needs consent the
run pauses on the approval gate: `ApprovalService` persists the request, the scheduler moves the
run to `blocked`, emits an `approval` event, and waits until the user decides or 60 seconds
pass (PASSPORT §11, kernel README "Permissions and approvals").

The model has no way to ask the user anything mid-turn. When a request is ambiguous it guesses,
or ends the turn with a question in prose, and the user answers in a new turn that has lost the
tool results of the previous one.

This task adds `chat.ask`: a call the model makes with up to six questions — free text, a single
choice or a multiple choice, each option optionally marked as recommended. The turn pauses, the
thread shows an answer card, and the answers return to the model as the call's result in the
same turn. It is a **bridge, not a tool**: it is not in `ToolRegistry`, not on «Инструменты»,
not admitted by `PermissionService`, has no step row and does not count against the call budget.
It is a second gate in the kernel beside the approval gate.

## Why

Without it every ambiguity costs a turn and its context, and the model either guesses wrong or
answers with a wall of prose questions the user retypes. The agentic engine (TASK_040) needs the
same primitive to ask during a plan.

## Depends on

- `TASK_044` — the chat thread and tool call cards this card sits beside.
- `TASK_036_EXTRA_3` — the approval card pattern inside a tool call (`ChatToolCalls.tsx`).

## Read first

- `/docs/PASSPORT.md` §7 (event channel, error model), §11 (approval gate), §18.6 (locale)
- `crates/zvs-host/src/kernel/README.md` — "Permissions and approvals", "Chat"
- `crates/zvs-host/src/services/approvals.rs`, `kernel/scheduler.rs` (`NodeLink::request_approval`)
- `crates/zvs-host/src/services/chat.rs` — `respond`, `call_tool`

## Scope

**In scope**

*Contract* (`zvs_contract::questions`, `packages/shared/src/questions.ts`, `fixtures/questions/`)
- `QuestionKind` `text | single | multiple`; `QuestionOptionDto { id, label, description?,
  recommended }`; `QuestionDto { id, title, kind, options, allowOther }`;
  `QuestionRequestDto { id, runId, conversationId?, questions }`; `QuestionAnswerDto
  { questionId, selected, text? }`; `QuestionRecordDto { request, answers, skipped }` as the
  call's meta; `ChatAnswerInput { id, requestId, answers }` and `ChatSkipInput { id, requestId }`.
- Limits: 1–6 questions; a choice question has 2–8 options, a text question none; at most one
  recommended option per question; ids unique.
- `HostEvent` gains `question { request }`. `ChatToolCallStatus` gains `awaiting-answer`.
- Channels `chat.answer` and `chat.skip`.

*Kernel*
- `StepLink::ask_user` / `StepContext::ask_user`, `QuestionHook` on `SchedulerOptions`. The
  scheduler counts an open question like an approval: the run is `blocked`, no new node is
  scheduled, the `question` event is emitted, and the run returns to `running` once answered.
- `QuestionService` (`services/questions.rs`), owned by `RunService`: in-memory waiters keyed by
  request id, **no timeout**; cancellation resolves the wait as cancelled. `answer` checks the
  run id and validates the answers against the request; `skip` resolves without answers. Stale
  or repeated answers are `CONFLICT`.

*Chat*
- `chat.ask` is offered in every turn whose model can call tools, like the skill tools. The
  tool description is Russian; a short English prompt section says when to ask.
- `respond` intercepts the call before `call_tool`: arguments are normalised (ids assigned by the
  host) and validated; a malformed call returns the error as the result so the model can retry.
  It is a barrier like a call with side effects; a second `chat.ask` in the round is refused.
- The call record is `awaiting-answer` with the request as meta, then `succeeded` with the
  answers (or `skipped`) in meta. The model gets a plain-text summary of the answers.
- An OS notification for a question in a background dialog, under the approvals setting.

*Renderer*
- `ChatStore` takes `question` events, `answerQuestion` / `skipQuestion`.
- `ChatQuestionCard`: one block per question — textarea, radio list or checkbox list; the
  recommended option first with a «Рекомендуется» badge; «Другое» with a text field when
  `allowOther`; «Ответить» and «Пропустить». A finished call shows the answers read-only; an
  `awaiting-answer` call outside a live run shows «Вопрос остался без ответа».
- Copy in `questionCopy.ts`.

**Out of scope** (do not do these, even if tempting)
- Persisting open questions across a restart: a restart ends the turn, as for approvals.
- A per-dialog switch to turn `chat.ask` off.
- Asking from scenarios or the agentic engine (the kernel primitive is ready for them).
- Answering from the composer.

## Decisions (answered by the user before the task was written)

- Offered always, like the skill tools — no switch.
- No timeout: only an answer, a skip or a cancel ends the wait.
- At most 6 questions per call.

## Verify

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
pnpm typecheck && pnpm lint && pnpm test && pnpm format:check
```

Tests that must exist:
- Contract: fixtures for every new DTO, both sides byte-equal.
- `QuestionService`: answer resolves the waiter; wrong run, unknown id, repeated answer conflict;
  answers are validated (unknown option, two options on a single choice, empty text); skip;
  cancellation resolves the wait.
- Kernel: an open question moves the run to `blocked`, emits `question`, and back to `running`.
- Chat: a model call of `chat.ask` blocks the turn, the answer reaches the model as the call
  result in the same turn, the record ends `succeeded` with the answers; malformed arguments go
  back to the model; a cancelled turn marks the call `cancelled`; `chat.ask` is not counted in
  the call budget and is not on the tool list.
- Renderer: the store turns a `question` event into a pending card, sends `chat.answer` with the
  selected option ids and text, and `chat.skip`.

## Definition of done

- [x] Contract, fixtures, `question` event, `awaiting-answer`, `chat.answer`, `chat.skip`
- [x] Question gate in the kernel and `QuestionService` with no timeout
- [x] `chat.ask` offered to every tool-capable turn and intercepted in the loop
- [x] Answer card, read-only answered state, notification
- [x] Gates pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
- Contract `zvs_contract::questions` / `packages/shared/src/questions.ts`: `QuestionKind`,
  `QuestionOptionDto`, `QuestionDto`, `QuestionRequestDto`, `QuestionAnswerDto`,
  `QuestionRecordDto`, `ChatAnswerInput`, `ChatSkipInput`, fixtures in `fixtures/questions/`.
  `HostEvent` gains `question` (fixture added, the variant test lists it); `ChatToolCallStatus`
  gains `awaiting-answer`. Channels `chat.answer` and `chat.skip`.
- Kernel: `QuestionOutcome`, `StepLink::ask_user` (default: refused with
  `NO_QUESTION_SERVICE`), `StepContext::ask_user`, `QuestionHook` on `SchedulerOptions`.
  `NodeLink::ask_user` validates the request, counts it in `pending_gates` (renamed from
  `pending_approvals`, shared with approvals), sets `blocked`, emits `question`, and resumes.
- `services/questions.rs`: `QuestionService` — in-memory waiters, no timeout, cancellation →
  `RUN_CANCELLED`; `answer` / `skip` conflict unless pending for that run; `checked_answers`
  orders the answers by question, trims text, and rejects unknown questions or options, repeats,
  two picks on `single`, text without `allowOther`, and empty answers (Russian messages, since
  the store shows them). Owned by `RunService` (`runs.questions()`).
- `services/chat_ask.rs`: the Russian description and schema of `chat.ask` (wire name
  `ask_user`), the English prompt section, `ask_request` (host-assigned `q1…`/`o1…` ids, a
  missing kind inferred, a malformed call explained to the model), `answers_text` for the result.
- Chat: `chat.prepare` and `chat.context` add the tool for tool-capable models; `respond` does
  not count it in the call budget and refuses a second one in a round; `call_tool` intercepts it,
  records `awaiting-answer` with a `QuestionRecordDto` meta, waits on the gate, and settles
  `succeeded` with the answers or `skipped`. A cancelled wait marks the call `cancelled`.
- Notifications: a `question` in a chat raises «Требуется ответ» with the first question.
- Renderer: `questionCopy.ts` (copy, draft logic, read-back), `ChatQuestionCard` (radio,
  checkbox and text fields, recommended first with «Рекомендуется», «Другое»,
  «Ответить»/«Пропустить») and `QuestionAnswers` (read-only); `ChatToolCalls` shows the card in
  the live call, the answers in history; `ChatStore.questions`, `answerQuestion`,
  `skipQuestion`; the generation phase reads «Ожидает ответа».
- PASSPORT §7 and §11 and the kernel README describe the gate and the bridge.
- Tests: contract unit tests and fixtures; `services/questions/tests.rs`; five loop tests in
  `chat/tests/tool_loop.rs` (answer, skip, malformed and second call, cancel, no tool calling);
  notification and IPC tests; `tests/renderer/chatQuestions.test.ts`.

**Deviated from plan:**
- Calls of a round run in the model's order; `chat.ask` is a barrier rather than being moved last.
- The card lives inside the `chat.ask` call card, not as a separate block above the composer.
- The toast uses the approvals notification setting rather than a new one.
- Existing chat tests that asserted «no tools offered» now assert that only `chat.ask` is
  offered; a model without tool calling still gets none.

**Next session should know:**
- `chat.ask` adds its description, schema and prompt section to every tool-capable turn, so
  small context windows pay for it on each turn; `chat.context` counts it unless the model is
  known not to call tools.
- Open questions are not persisted: a restart interrupts the run, and history shows the call as
  «Вопрос остался без ответа». A renderer that loses the run handle can still end a waiting run
  from «Задачи».
- TASK_040 can reuse the gate through `StepContext::ask_user`.
- Not run here: `pnpm build`, `pnpm test:e2e`. `pnpm typecheck` still fails on
  `tests/renderer/fileChanges.test.ts:59` (`from: undefined` under exact optional types), as on
  the clean tree.
