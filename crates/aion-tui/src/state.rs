use std::collections::HashSet;

use aion_agent::commands::CommandSpec;
use aion_types::message::{Message, TokenUsage};

use crate::command_popup::CommandPopup;
use crate::composer::Composer;
use crate::event::AgentEvent;
use crate::transcript::{EntryKind, TranscriptEntry, entries_from_messages};

#[derive(Debug)]
pub(super) struct ApprovalRequest {
    pub(super) call_id: String,
    pub(super) name: String,
    pub(super) description: String,
    pub(super) input: String,
}

#[derive(Debug)]
pub(super) struct AppState {
    pub(super) model: String,
    pub(super) provider: String,
    pub(super) cwd: String,
    pub(super) session_id: Option<String>,
    pub(super) no_color: bool,
    pub(super) composer: Composer,
    pub(super) popup: CommandPopup,
    pub(super) transcript: Vec<TranscriptEntry>,
    pub(super) approval: Option<ApprovalRequest>,
    pub(super) busy: bool,
    pub(super) spinner_frame: usize,
    pub(super) usage: TokenUsage,
    pub(super) turns: usize,
    pub(super) scroll_back: usize,
    active_assistant: Option<usize>,
    active_thinking: Option<usize>,
    protocol_results: HashSet<String>,
}

impl AppState {
    pub(super) fn new(model: String, provider: String, cwd: String, no_color: bool) -> Self {
        Self {
            model,
            provider,
            cwd,
            session_id: None,
            no_color,
            composer: Composer::default(),
            popup: CommandPopup::default(),
            transcript: Vec::new(),
            approval: None,
            busy: false,
            spinner_frame: 0,
            usage: TokenUsage::default(),
            turns: 0,
            scroll_back: 0,
            active_assistant: None,
            active_thinking: None,
            protocol_results: HashSet::new(),
        }
    }

    pub(super) fn set_commands(&mut self, commands: Vec<CommandSpec>) {
        self.popup.set_commands(commands);
    }

    pub(super) fn set_history(&mut self, messages: &[Message]) {
        self.transcript = entries_from_messages(messages);
        self.active_assistant = None;
        self.active_thinking = None;
    }

    pub(super) fn begin_turn(&mut self, input: &str) {
        self.busy = true;
        self.scroll_back = 0;
        self.active_assistant = None;
        self.active_thinking = None;
        if !self.popup.recognizes(input) {
            self.transcript.push(TranscriptEntry::new(EntryKind::User, "", input));
        }
    }

    pub(super) fn finish_turn(&mut self, turns: usize, usage: TokenUsage) {
        self.busy = false;
        self.turns = turns;
        self.usage = usage;
        self.active_assistant = None;
        self.active_thinking = None;
    }

    pub(super) fn cancel_turn(&mut self) {
        self.busy = false;
        self.active_assistant = None;
        self.active_thinking = None;
        self.transcript.push(TranscriptEntry::new(
            EntryKind::Info,
            "Stopped",
            "Turn cancelled by user",
        ));
    }

    pub(super) fn tick(&mut self) {
        if self.busy {
            self.spinner_frame = (self.spinner_frame + 1) % 4;
        }
    }

    pub(super) fn handle_agent_event(&mut self, event: AgentEvent) {
        self.scroll_back = 0;
        match event {
            AgentEvent::StreamStart => {
                self.active_assistant = None;
                self.active_thinking = None;
            }
            AgentEvent::TextDelta(text) => self.append_stream(EntryKind::Assistant, "", text),
            AgentEvent::Thinking(text) => self.append_stream(EntryKind::Thinking, "Thinking", text),
            AgentEvent::Info(text) => self
                .transcript
                .push(TranscriptEntry::new(EntryKind::Info, "Info", text)),
            AgentEvent::Error(text) => self
                .transcript
                .push(TranscriptEntry::new(EntryKind::Error, "Error", text)),
            AgentEvent::ToolCall { name, input } => {
                self.transcript.push(TranscriptEntry::new(EntryKind::Tool, name, input));
            }
            AgentEvent::ToolResult {
                call_id,
                name,
                is_error,
                content,
            } => {
                if self.protocol_results.remove(&call_id) {
                    return;
                }
                let kind = if is_error { EntryKind::Error } else { EntryKind::Tool };
                self.transcript.push(TranscriptEntry::new(kind, name, content));
            }
            AgentEvent::ProtocolToolResult {
                call_id,
                name,
                is_error,
                content,
            } => {
                self.protocol_results.insert(call_id);
                let kind = if is_error { EntryKind::Error } else { EntryKind::Tool };
                self.transcript.push(TranscriptEntry::new(kind, name, content));
            }
            AgentEvent::ApprovalRequested {
                call_id,
                name,
                description,
                input,
            } => {
                self.approval = Some(ApprovalRequest {
                    call_id,
                    name,
                    description,
                    input,
                });
            }
            AgentEvent::ToolRunning { call_id, name } => {
                if self.approval.as_ref().is_some_and(|request| request.call_id == call_id) {
                    self.approval = None;
                }
                self.transcript
                    .push(TranscriptEntry::new(EntryKind::Tool, name, "Running…"));
            }
            AgentEvent::ToolCancelled { call_id, name, reason } => {
                if self.approval.as_ref().is_some_and(|request| request.call_id == call_id) {
                    self.approval = None;
                }
                self.transcript.push(TranscriptEntry::new(
                    EntryKind::Error,
                    name,
                    format!("Cancelled: {reason}"),
                ));
            }
        }
    }

    fn append_stream(&mut self, kind: EntryKind, label: &str, text: String) {
        let active = match kind {
            EntryKind::Assistant => &mut self.active_assistant,
            EntryKind::Thinking => &mut self.active_thinking,
            _ => return,
        };
        if let Some(index) = *active {
            self.transcript[index].text.push_str(&text);
        } else {
            self.transcript.push(TranscriptEntry::new(kind, label, text));
            *active = Some(self.transcript.len() - 1);
        }
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
