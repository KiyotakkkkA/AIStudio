use std::sync::Arc;

use serde_json::json;
use tokio_util::sync::CancellationToken;
use zvs_contract::{AppErrorCode, QuestionAnswerDto, QuestionRequestDto};

use super::*;

const RUN: &str = "0199aa11-1111-7111-8111-111111111111";
const OTHER_RUN: &str = "0199aa11-4444-7444-8444-444444444444";
const REQUEST: &str = "0199aa11-2222-7222-8222-222222222222";

fn request() -> QuestionRequestDto {
    serde_json::from_value(json!({
        "id": REQUEST,
        "runId": RUN,
        "questions": [
            {
                "id": "q1",
                "title": "Формат",
                "kind": "single",
                "options": [
                    { "id": "o1", "label": "DOCX", "recommended": true },
                    { "id": "o2", "label": "PDF", "recommended": false }
                ],
                "allowOther": true
            },
            {
                "id": "q2",
                "title": "Разделы",
                "kind": "multiple",
                "options": [
                    { "id": "o1", "label": "Введение", "recommended": false },
                    { "id": "o2", "label": "Выводы", "recommended": false }
                ],
                "allowOther": false
            },
            { "id": "q3", "title": "Название", "kind": "text", "options": [], "allowOther": false }
        ]
    }))
    .expect("a request")
}

fn answer(question: &str, selected: &[&str], text: Option<&str>) -> QuestionAnswerDto {
    QuestionAnswerDto {
        question_id: question.to_owned(),
        selected: selected.iter().map(|id| (*id).to_owned()).collect(),
        text: text.map(str::to_owned),
    }
}

async fn asked(
    service: &Arc<QuestionService>,
) -> tokio::task::JoinHandle<Result<QuestionOutcome, AppError>> {
    let waiting = Arc::clone(service);
    let handle =
        tokio::spawn(async move { waiting.wait(request(), &CancellationToken::new()).await });
    while service.pending(RUN).is_empty() {
        tokio::task::yield_now().await;
    }
    handle
}

#[tokio::test]
async fn an_answer_resolves_the_waiter_in_question_order_with_trimmed_text() {
    let service = Arc::new(QuestionService::new());
    let handle = asked(&service).await;
    service
        .answer(
            RUN,
            REQUEST,
            &[
                answer("q3", &[], Some("  Итоги  ")),
                answer("q1", &["o2"], None),
            ],
        )
        .expect("accepted");
    let outcome = handle.await.expect("joined").expect("answered");
    assert_eq!(
        outcome,
        QuestionOutcome::Answered(vec![
            answer("q1", &["o2"], None),
            answer("q3", &[], Some("Итоги")),
        ])
    );
    assert!(service.pending(RUN).is_empty());
}

#[tokio::test]
async fn a_wrong_run_an_unknown_request_and_a_second_answer_conflict() {
    let service = Arc::new(QuestionService::new());
    let handle = asked(&service).await;
    let answers = [answer("q1", &["o1"], None)];
    for (run, request) in [(OTHER_RUN, REQUEST), (RUN, OTHER_RUN)] {
        let error = service
            .answer(run, request, &answers)
            .expect_err("not pending");
        assert_eq!(error.code, AppErrorCode::Conflict);
        assert_eq!(error.message, QUESTION_NOT_PENDING);
    }
    service.answer(RUN, REQUEST, &answers).expect("accepted");
    let error = service
        .answer(RUN, REQUEST, &answers)
        .expect_err("already answered");
    assert_eq!(error.code, AppErrorCode::Conflict);
    handle.await.expect("joined").expect("answered");
}

#[tokio::test]
async fn answers_are_checked_against_their_questions_and_a_rejected_answer_keeps_the_wait() {
    let service = Arc::new(QuestionService::new());
    let handle = asked(&service).await;
    let cases = [
        (vec![answer("q9", &[], Some("x"))], ANSWER_UNKNOWN_QUESTION),
        (
            vec![answer("q1", &["o1"], None), answer("q1", &["o2"], None)],
            ANSWER_REPEATED,
        ),
        (vec![answer("q1", &["o7"], None)], ANSWER_UNKNOWN_OPTION),
        (vec![answer("q1", &["o1", "o2"], None)], ANSWER_ONE_OPTION),
        (vec![answer("q1", &["o1"], Some("HTML"))], ANSWER_ONE_OPTION),
        (vec![answer("q2", &["o1"], Some("Своё"))], ANSWER_NO_OTHER),
        (vec![answer("q2", &["o1", "o1"], None)], ANSWER_REPEATED),
        (vec![answer("q2", &[], None)], ANSWER_EMPTY),
        (vec![answer("q3", &[], Some("   "))], ANSWER_EMPTY),
        (
            vec![answer("q3", &["o1"], Some("x"))],
            ANSWER_UNKNOWN_OPTION,
        ),
    ];
    for (answers, message) in cases {
        let error = service
            .answer(RUN, REQUEST, &answers)
            .expect_err("rejected");
        assert_eq!(error.code, AppErrorCode::ValidationFailed);
        assert_eq!(error.message, message);
    }
    assert_eq!(service.pending(RUN).len(), 1);
    service
        .answer(
            RUN,
            REQUEST,
            &[
                answer("q1", &[], Some("HTML")),
                answer("q2", &["o1", "o2"], None),
            ],
        )
        .expect("an other answer and a multiple choice");
    handle.await.expect("joined").expect("answered");
}

#[tokio::test]
async fn a_skip_resolves_without_answers() {
    let service = Arc::new(QuestionService::new());
    let handle = asked(&service).await;
    service.skip(RUN, REQUEST).expect("skipped");
    assert_eq!(
        handle.await.expect("joined").expect("resolved"),
        QuestionOutcome::Skipped
    );
    assert_eq!(
        service.skip(RUN, REQUEST).expect_err("gone").code,
        AppErrorCode::Conflict
    );
}

#[tokio::test]
async fn cancellation_ends_the_wait_and_forgets_the_request() {
    let service = Arc::new(QuestionService::new());
    let cancel = CancellationToken::new();
    let waiting = Arc::clone(&service);
    let token = cancel.clone();
    let handle = tokio::spawn(async move { waiting.wait(request(), &token).await });
    while service.pending(RUN).is_empty() {
        tokio::task::yield_now().await;
    }
    cancel.cancel();
    let error = handle.await.expect("joined").expect_err("cancelled");
    assert_eq!(error.code, AppErrorCode::RunCancelled);
    assert!(service.pending(RUN).is_empty());
    assert_eq!(
        service.skip(RUN, REQUEST).expect_err("gone").code,
        AppErrorCode::Conflict
    );
}
