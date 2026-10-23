use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, MutexGuard, PoisonError};

use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use zvs_contract::validate::js_trim;
use zvs_contract::{
    AppError, AppErrorCode, QuestionAnswerDto, QuestionDto, QuestionKind, QuestionRequestDto,
};

use crate::kernel::QuestionOutcome;
use crate::kernel::types::cancelled;

pub const QUESTION_NOT_PENDING: &str = "Вопрос больше не ожидает ответа";
pub const ANSWER_UNKNOWN_QUESTION: &str = "Ответ относится к вопросу, которого нет в запросе";
pub const ANSWER_REPEATED: &str = "На один вопрос дано несколько ответов";
pub const ANSWER_UNKNOWN_OPTION: &str = "Выбран вариант, которого нет в вопросе";
pub const ANSWER_ONE_OPTION: &str = "В этом вопросе выбирается один вариант";
pub const ANSWER_NO_OTHER: &str = "В этом вопросе нельзя указать свой вариант";
pub const ANSWER_EMPTY: &str = "Ответ не заполнен";

struct Waiter {
    run_id: String,
    request: QuestionRequestDto,
    sender: oneshot::Sender<QuestionOutcome>,
}

#[derive(Default)]
pub struct QuestionService {
    waiting: Mutex<HashMap<String, Waiter>>,
}

fn rejected(message: &str, question: &str) -> AppError {
    AppError::new(AppErrorCode::ValidationFailed, message).with_detail("questionId", question)
}

fn checked_answer(
    question: &QuestionDto,
    answer: &QuestionAnswerDto,
) -> Result<QuestionAnswerDto, AppError> {
    let text = answer
        .text
        .as_deref()
        .map(js_trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned);
    let mut seen = HashSet::new();
    for id in &answer.selected {
        if !question.options.iter().any(|option| &option.id == id) {
            return Err(rejected(ANSWER_UNKNOWN_OPTION, &question.id));
        }
        if !seen.insert(id.as_str()) {
            return Err(rejected(ANSWER_REPEATED, &question.id));
        }
    }
    match question.kind {
        QuestionKind::Text => {
            if !answer.selected.is_empty() {
                return Err(rejected(ANSWER_UNKNOWN_OPTION, &question.id));
            }
            if text.is_none() {
                return Err(rejected(ANSWER_EMPTY, &question.id));
            }
        }
        QuestionKind::Single | QuestionKind::Multiple => {
            if text.is_some() && !question.allow_other {
                return Err(rejected(ANSWER_NO_OTHER, &question.id));
            }
            let picked = answer.selected.len() + usize::from(text.is_some());
            if picked == 0 {
                return Err(rejected(ANSWER_EMPTY, &question.id));
            }
            if question.kind == QuestionKind::Single && picked > 1 {
                return Err(rejected(ANSWER_ONE_OPTION, &question.id));
            }
        }
    }
    Ok(QuestionAnswerDto {
        question_id: question.id.clone(),
        selected: answer.selected.clone(),
        text,
    })
}

pub fn checked_answers(
    request: &QuestionRequestDto,
    answers: &[QuestionAnswerDto],
) -> Result<Vec<QuestionAnswerDto>, AppError> {
    let mut seen = HashSet::new();
    for answer in answers {
        if !request
            .questions
            .iter()
            .any(|question| question.id == answer.question_id)
        {
            return Err(rejected(ANSWER_UNKNOWN_QUESTION, &answer.question_id));
        }
        if !seen.insert(answer.question_id.as_str()) {
            return Err(rejected(ANSWER_REPEATED, &answer.question_id));
        }
    }
    request
        .questions
        .iter()
        .filter_map(|question| {
            answers
                .iter()
                .find(|answer| answer.question_id == question.id)
                .map(|answer| checked_answer(question, answer))
        })
        .collect()
}

impl QuestionService {
    pub fn new() -> Self {
        Self::default()
    }

    fn waiting(&self) -> MutexGuard<'_, HashMap<String, Waiter>> {
        self.waiting.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub async fn wait(
        &self,
        request: QuestionRequestDto,
        cancel: &CancellationToken,
    ) -> Result<QuestionOutcome, AppError> {
        let id = request.id.clone();
        let (sender, receiver) = oneshot::channel();
        self.waiting().insert(
            id.clone(),
            Waiter {
                run_id: request.run_id.to_string(),
                request,
                sender,
            },
        );
        let outcome = tokio::select! {
            biased;
            answered = receiver => answered.ok(),
            () = cancel.cancelled() => None,
        };
        self.waiting().remove(&id);
        outcome.ok_or_else(cancelled)
    }

    pub fn pending(&self, run_id: &str) -> Vec<QuestionRequestDto> {
        self.waiting()
            .values()
            .filter(|waiter| waiter.run_id == run_id)
            .map(|waiter| waiter.request.clone())
            .collect()
    }

    fn resolve(
        &self,
        run_id: &str,
        request_id: &str,
        outcome: impl FnOnce(&QuestionRequestDto) -> Result<QuestionOutcome, AppError>,
    ) -> Result<(), AppError> {
        let mut waiting = self.waiting();
        let Some(waiter) = waiting
            .get(request_id)
            .filter(|waiter| waiter.run_id == run_id)
        else {
            return Err(AppError::new(AppErrorCode::Conflict, QUESTION_NOT_PENDING));
        };
        let outcome = outcome(&waiter.request)?;
        if let Some(waiter) = waiting.remove(request_id) {
            let _ = waiter.sender.send(outcome);
        }
        Ok(())
    }

    pub fn answer(
        &self,
        run_id: &str,
        request_id: &str,
        answers: &[QuestionAnswerDto],
    ) -> Result<(), AppError> {
        self.resolve(run_id, request_id, |request| {
            checked_answers(request, answers).map(QuestionOutcome::Answered)
        })
    }

    pub fn skip(&self, run_id: &str, request_id: &str) -> Result<(), AppError> {
        self.resolve(run_id, request_id, |_| Ok(QuestionOutcome::Skipped))
    }
}

#[cfg(test)]
mod tests;
