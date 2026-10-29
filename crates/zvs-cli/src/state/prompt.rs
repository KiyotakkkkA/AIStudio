use serde_json::Value;
use zvs_contract::{
    ApprovalRequestDto, ExecMode, FileDiffDto, FilesApprovalDto, FolderApprovalDto,
    ProcessApprovalDto, QuestionAnswerDto, QuestionKind, QuestionRequestDto, RunId,
};

use super::input::Input;
use crate::copy;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalDetail {
    Plain,
    Process {
        argv: Vec<String>,
        cwd: String,
        scripted: bool,
    },
    Files {
        changes: Vec<FileDiffDto>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    AccessConversation,
    AccessAlways,
    AccessDeny,
    Approve,
    AlwaysHere,
    AlwaysEverywhere,
    Deny,
    FolderOnce,
    FolderConversation,
    FolderAlways,
    FolderDeny,
}

impl Choice {
    pub const fn label(self, files: bool) -> &'static str {
        match self {
            Self::AccessConversation => copy::ACCESS_CONVERSATION,
            Self::AccessAlways => copy::ACCESS_ALWAYS,
            Self::AccessDeny => copy::ACCESS_DENY,
            Self::Approve => copy::APPROVE,
            Self::AlwaysHere if files => copy::APPROVE_FILES_HERE,
            Self::AlwaysHere => copy::APPROVE_ALWAYS_HERE,
            Self::AlwaysEverywhere if files => copy::APPROVE_FILES_EXACT,
            Self::AlwaysEverywhere => copy::APPROVE_ALWAYS_EVERYWHERE,
            Self::Deny | Self::FolderDeny => copy::DENY,
            Self::FolderOnce => copy::FOLDER_ONCE,
            Self::FolderConversation => copy::FOLDER_CONVERSATION,
            Self::FolderAlways => copy::FOLDER_ALWAYS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionState {
    pub request: QuestionRequestDto,
    pub index: usize,
    pub answers: Vec<QuestionAnswerDto>,
    pub checked: Vec<bool>,
    pub other: Option<Input>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptKind {
    Access {
        folder: String,
    },
    Approval {
        run: RunId,
        id: String,
        subject: String,
        detail: ApprovalDetail,
        rules: bool,
    },
    Folder {
        run: RunId,
        id: String,
        folders: Vec<String>,
    },
    Question(Box<QuestionState>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    pub kind: PromptKind,
    pub selected: usize,
}

pub fn rules_apply(mode: ExecMode, detail: &ApprovalDetail) -> bool {
    mode == ExecMode::AutoBypass
        && match detail {
            ApprovalDetail::Plain => false,
            ApprovalDetail::Process { scripted, argv, .. } => !scripted && !argv.is_empty(),
            ApprovalDetail::Files { changes } => !changes.is_empty(),
        }
}

pub fn from_request(request: &ApprovalRequestDto, mode: ExecMode) -> Option<Prompt> {
    let value = Value::Object(request.clone());
    if let Ok(folder) = serde_json::from_value::<FolderApprovalDto>(value.clone()) {
        return Some(Prompt::new(PromptKind::Folder {
            run: folder.run_id,
            id: folder.id,
            folders: folder.folders.into_iter().map(|entry| entry.path).collect(),
        }));
    }
    let detail = if let Ok(files) = serde_json::from_value::<FilesApprovalDto>(value.clone()) {
        ApprovalDetail::Files {
            changes: files.changes,
        }
    } else if let Ok(process) = serde_json::from_value::<ProcessApprovalDto>(value.clone()) {
        ApprovalDetail::Process {
            argv: process.process.argv,
            cwd: process.process.cwd,
            scripted: process.process.language.is_some(),
        }
    } else {
        ApprovalDetail::Plain
    };
    let id = request.get("id")?.as_str()?.to_owned();
    let subject = request.get("subject")?.as_str()?.to_owned();
    let run = serde_json::from_value(request.get("runId")?.clone()).ok()?;
    let rules = rules_apply(mode, &detail);
    Some(Prompt::new(PromptKind::Approval {
        run,
        id,
        subject,
        detail,
        rules,
    }))
}

pub fn question(request: QuestionRequestDto) -> Prompt {
    let mut state = QuestionState {
        request,
        index: 0,
        answers: Vec::new(),
        checked: Vec::new(),
        other: None,
    };
    state.prepare();
    Prompt::new(PromptKind::Question(Box::new(state)))
}

impl QuestionState {
    pub fn current(&self) -> Option<&zvs_contract::QuestionDto> {
        self.request.questions.get(self.index)
    }

    pub fn option_count(&self) -> usize {
        self.current().map_or(0, |question| {
            question.options.len() + usize::from(question.allow_other)
        })
    }

    fn prepare(&mut self) {
        let Some(question) = self.current() else {
            return;
        };
        let count = question.options.len();
        let text = question.kind == QuestionKind::Text;
        self.checked = vec![false; count];
        self.other = text.then(Input::default);
    }

    pub fn is_other(&self, selected: usize) -> bool {
        self.current()
            .is_some_and(|question| question.allow_other && selected == question.options.len())
    }

    pub fn toggle(&mut self, selected: usize) {
        if let Some(checked) = self.checked.get_mut(selected) {
            *checked = !*checked;
        }
    }

    pub fn commit(&mut self, selected: usize) -> bool {
        let Some(question) = self.current().cloned() else {
            return true;
        };
        if self.other.is_none() && self.is_other(selected) {
            self.other = Some(Input::default());
            return false;
        }
        let text = self
            .other
            .as_mut()
            .map(Input::take)
            .filter(|text| !text.is_empty());
        let selected_ids: Vec<String> = match question.kind {
            QuestionKind::Text => Vec::new(),
            QuestionKind::Single => question
                .options
                .get(selected)
                .filter(|_| text.is_none())
                .map(|option| vec![option.id.clone()])
                .unwrap_or_default(),
            QuestionKind::Multiple => question
                .options
                .iter()
                .zip(&self.checked)
                .filter(|(_, checked)| **checked)
                .map(|(option, _)| option.id.clone())
                .collect(),
        };
        if selected_ids.is_empty() && text.is_none() {
            return false;
        }
        self.answers.push(QuestionAnswerDto {
            question_id: question.id,
            selected: selected_ids,
            text,
        });
        self.index += 1;
        self.prepare();
        self.index >= self.request.questions.len()
    }
}

impl Prompt {
    pub const fn new(kind: PromptKind) -> Self {
        Self { kind, selected: 0 }
    }

    pub fn access(folder: &str) -> Self {
        Self::new(PromptKind::Access {
            folder: folder.to_owned(),
        })
    }

    pub fn choices(&self) -> Vec<Choice> {
        match &self.kind {
            PromptKind::Access { .. } => vec![
                Choice::AccessConversation,
                Choice::AccessAlways,
                Choice::AccessDeny,
            ],
            PromptKind::Approval { rules, .. } => {
                let mut choices = vec![Choice::Approve];
                if *rules {
                    choices.extend([Choice::AlwaysHere, Choice::AlwaysEverywhere]);
                }
                choices.push(Choice::Deny);
                choices
            }
            PromptKind::Folder { .. } => vec![
                Choice::FolderOnce,
                Choice::FolderConversation,
                Choice::FolderAlways,
                Choice::FolderDeny,
            ],
            PromptKind::Question(_) => Vec::new(),
        }
    }

    pub fn count(&self) -> usize {
        match &self.kind {
            PromptKind::Question(state) => state.option_count(),
            _ => self.choices().len(),
        }
    }

    pub fn up(&mut self) {
        let count = self.count();
        if count > 0 {
            self.selected = (self.selected + count - 1) % count;
        }
    }

    pub fn down(&mut self) {
        let count = self.count();
        if count > 0 {
            self.selected = (self.selected + 1) % count;
        }
    }

    pub fn choice(&self) -> Option<Choice> {
        self.choices().get(self.selected).copied()
    }

    pub const fn is_files(&self) -> bool {
        matches!(
            self.kind,
            PromptKind::Approval {
                detail: ApprovalDetail::Files { .. },
                ..
            }
        )
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use zvs_contract::QuestionKind;

    use super::*;

    const RUN: &str = "0199aa11-1111-7111-8111-111111111111";
    const APPROVAL: &str = "0199aa11-2222-7111-8111-111111111111";
    const CONVERSATION: &str = "0199aa11-3333-7111-8111-111111111111";

    fn object(value: Value) -> ApprovalRequestDto {
        let Value::Object(map) = value else {
            unreachable!()
        };
        map
    }

    fn process(language: Option<&str>) -> ApprovalRequestDto {
        let mut plan = json!({
            "program": "run_command",
            "argv": ["deploytool", "sync"],
            "cwd": "D:\\work",
            "packages": [],
        });
        if let Some(language) = language {
            plan["language"] = json!(language);
            plan["source"] = json!("print(1)");
        }
        object(json!({
            "id": APPROVAL,
            "runId": RUN,
            "kind": "process",
            "subject": "commands.run",
            "scope": "global",
            "expiresAt": 1,
            "conversationId": CONVERSATION,
            "tool": "run_command",
            "action": "Run",
            "process": plan,
        }))
    }

    #[test]
    fn always_allow_is_offered_only_for_plain_commands_in_auto_bypass() {
        let prompt = from_request(&process(None), ExecMode::AutoBypass).expect("prompt");
        assert_eq!(
            prompt.choices(),
            [
                Choice::Approve,
                Choice::AlwaysHere,
                Choice::AlwaysEverywhere,
                Choice::Deny
            ]
        );
        let ask = from_request(&process(None), ExecMode::SafetyAsk).expect("prompt");
        assert_eq!(ask.choices(), [Choice::Approve, Choice::Deny]);
        let script = from_request(&process(Some("python")), ExecMode::AutoBypass).expect("prompt");
        assert_eq!(script.choices(), [Choice::Approve, Choice::Deny]);
    }

    #[test]
    fn a_folder_request_offers_the_four_folder_answers() {
        let request = object(json!({
            "id": APPROVAL,
            "runId": RUN,
            "kind": "folder",
            "subject": "folder",
            "scope": "global",
            "expiresAt": 1,
            "conversationId": CONVERSATION,
            "tool": "read_file",
            "action": "Read",
            "folders": [{ "path": "D:\\other", "creates": [] }],
        }));
        let prompt = from_request(&request, ExecMode::AlwaysAsk).expect("prompt");
        assert!(
            matches!(&prompt.kind, PromptKind::Folder { folders, .. } if folders == &["D:\\other"])
        );
        assert_eq!(prompt.count(), 4);
    }

    #[test]
    fn a_plain_approval_still_needs_an_id_and_subject() {
        let request = object(json!({ "id": APPROVAL, "runId": RUN, "subject": "web.fetch" }));
        let prompt = from_request(&request, ExecMode::AutoBypass).expect("prompt");
        assert_eq!(prompt.choices(), [Choice::Approve, Choice::Deny]);
        assert!(from_request(&object(json!({ "runId": RUN })), ExecMode::AlwaysAsk).is_none());
    }

    fn questions() -> QuestionRequestDto {
        serde_json::from_value(json!({
            "id": "q",
            "runId": RUN,
            "questions": [
                {
                    "id": "color", "title": "Цвет?", "kind": "single", "allowOther": true,
                    "options": [
                        { "id": "red", "label": "Красный", "recommended": true },
                        { "id": "blue", "label": "Синий", "recommended": false }
                    ]
                },
                {
                    "id": "parts", "title": "Части?", "kind": "multiple", "allowOther": false,
                    "options": [
                        { "id": "a", "label": "A", "recommended": false },
                        { "id": "b", "label": "B", "recommended": false }
                    ]
                },
                { "id": "note", "title": "Заметка?", "kind": "text", "allowOther": false, "options": [] }
            ]
        }))
        .expect("parses")
    }

    #[test]
    fn questions_are_answered_one_by_one_with_other_text_and_checks() {
        let prompt = question(questions());
        let PromptKind::Question(mut state) = prompt.kind else {
            unreachable!()
        };
        assert_eq!(state.option_count(), 3);
        assert!(!state.commit(2));
        state.other.as_mut().expect("other").insert("Зелёный");
        assert!(!state.commit(2));
        assert_eq!(
            state.current().map(|q| q.kind),
            Some(QuestionKind::Multiple)
        );
        assert!(!state.commit(0));
        state.toggle(1);
        assert!(!state.commit(0));
        assert!(state.other.is_some());
        assert!(!state.commit(0));
        state.other.as_mut().expect("text").insert("спасибо");
        assert!(state.commit(0));
        assert_eq!(state.answers.len(), 3);
        assert_eq!(state.answers[0].text.as_deref(), Some("Зелёный"));
        assert!(state.answers[0].selected.is_empty());
        assert_eq!(state.answers[1].selected, ["b"]);
        assert_eq!(state.answers[2].text.as_deref(), Some("спасибо"));
    }
}
