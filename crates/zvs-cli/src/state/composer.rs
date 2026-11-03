use super::attachments::Attachment;
use super::modal::{Content, Modal};
use super::*;

impl App {
    pub fn can_select_model(&mut self, choice: &ModelChoice) -> bool {
        let has_images = self.input.has_images()
            || self.outgoing_attachments.iter().any(Attachment::image)
            || self
                .queue_editing
                .as_ref()
                .is_some_and(|(_, saved)| saved.has_images());
        if has_images && !self.supports_images_for(choice) {
            self.modal = Some(Modal::new(Content::Warning(
                copy::IMAGE_PROVIDER_WARNING.to_owned(),
            )));
            false
        } else {
            true
        }
    }

    pub fn clamp_modal(&mut self) {
        let (_, _, body) = crate::ui::modal::regions(ratatui::layout::Rect::new(
            0,
            0,
            self.viewport.0,
            self.viewport.1,
        ));
        let width = usize::from(body.width.max(1));
        let height = usize::from(body.height);
        let max = crate::ui::modal::content(self, width)
            .1
            .len()
            .saturating_sub(height);
        if let Some(modal) = &mut self.modal {
            modal.offset = modal.offset.min(max);
        }
    }

    pub fn supports_images_for(&self, choice: &ModelChoice) -> bool {
        self.providers.iter().any(|p| {
            p.summary.id == choice.provider
                && matches!(
                    p.summary.adapter,
                    zvs_contract::AdapterFamily::QwenWeb | zvs_contract::AdapterFamily::ZaiWeb
                )
        })
    }

    pub fn supports_images(&self) -> bool {
        self.model
            .as_ref()
            .is_some_and(|m| self.supports_images_for(m))
    }

    pub fn attachment_choices(&self) -> Vec<Attachment> {
        let Some((_, query)) = self.input.mention() else {
            return Vec::new();
        };
        let query = query.to_lowercase();
        let attached = self.input.attachments();
        self.attachment_options
            .iter()
            .filter(|a| {
                (!a.image() || self.supports_images())
                    && !attached.contains(a)
                    && a.name().to_lowercase().contains(&query)
            })
            .cloned()
            .collect()
    }

    pub fn attachments_open(&self) -> bool {
        self.palette_allowed() && self.input.mention().is_some()
    }

    pub fn choose_attachment(&mut self) {
        let choices = self.attachment_choices();
        if let Some(attachment) = choices.get(
            self.attachment_selected
                .min(choices.len().saturating_sub(1)),
        ) {
            let attached = self.input.attachments();
            let count = attached
                .iter()
                .filter(|a| a.image() == attachment.image())
                .count();
            if count
                >= if attachment.image() {
                    zvs_contract::IMAGES_MAX
                } else {
                    zvs_contract::chat::ATTACHED_STORES_MAX
                }
            {
                self.status = Some((Tone::Error, copy::ATTACHMENTS_LIMIT.to_owned()));
                return;
            }
            if let Some((start, _)) = self.input.mention() {
                self.input.attach(attachment.clone(), start);
            }
        }
        self.attachment_selected = 0;
    }

    pub fn stop_subagent(&mut self, id: RunId) -> Vec<Request> {
        self.status = Some((Tone::Info, copy::SUBAGENT_STOPPING.to_owned()));
        vec![Request::StopSubagent(id)]
    }

    pub fn subagents_command(&mut self, argument: &str) -> Vec<Request> {
        let (word, query) = argument
            .trim()
            .split_once(char::is_whitespace)
            .unwrap_or((argument.trim(), ""));
        let stop = word == "stop";
        if !word.is_empty() && !matches!(word, "list" | "stop") {
            self.status = Some((Tone::Error, copy::SUBAGENTS_USAGE.to_owned()));
            return Vec::new();
        }
        if query.trim().is_empty() {
            return self.open_panel(if stop {
                Panel::StopSubagents
            } else {
                Panel::Subagents
            });
        }
        let query = query.trim().to_lowercase();
        let calls = self.transcript.subagents();
        let found: Vec<_> = calls
            .iter()
            .filter(|call| {
                let run = transcript::subagent_run(call).unwrap_or_default();
                run.to_lowercase() == query
                    || call
                        .arguments
                        .get("task")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|task| task.to_lowercase() == query)
            })
            .collect();
        if let [call] = found.as_slice() {
            let run = transcript::subagent_run(call).unwrap_or_default();
            if stop {
                if transcript::subagent_running(call)
                    && let Some(run) = RunId::parse(run)
                {
                    return self.stop_subagent(run);
                }
                self.status = Some((Tone::Info, copy::SUBAGENT_NONE.to_owned()));
            } else {
                self.modal = Some(Modal::new(Content::Subagent(run.to_owned())));
            }
        } else {
            self.status = Some((Tone::Error, copy::SUBAGENT_NOT_FOUND.to_owned()));
        }
        Vec::new()
    }
}
