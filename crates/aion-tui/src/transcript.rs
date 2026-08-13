use aion_types::message::{ContentBlock, Message, Role};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EntryKind {
    User,
    Assistant,
    Thinking,
    Tool,
    Info,
    Error,
}

#[derive(Debug)]
pub(super) struct TranscriptEntry {
    pub(super) kind: EntryKind,
    pub(super) label: String,
    pub(super) text: String,
}

impl TranscriptEntry {
    pub(super) fn new(kind: EntryKind, label: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            kind,
            label: label.into(),
            text: text.into(),
        }
    }
}

pub(super) fn entries_from_messages(messages: &[Message]) -> Vec<TranscriptEntry> {
    let mut entries = Vec::new();
    for message in messages {
        for block in &message.content {
            match block {
                ContentBlock::Text { text } => match message.role {
                    Role::User => entries.push(TranscriptEntry::new(EntryKind::User, "", text)),
                    Role::Assistant => entries.push(TranscriptEntry::new(EntryKind::Assistant, "", text)),
                    Role::System => entries.push(TranscriptEntry::new(EntryKind::Info, "System", text)),
                    Role::Tool => entries.push(TranscriptEntry::new(EntryKind::Tool, "Tool", text)),
                },
                ContentBlock::Thinking { thinking, .. } => {
                    entries.push(TranscriptEntry::new(EntryKind::Thinking, "Thinking", thinking));
                }
                ContentBlock::ToolUse { name, input, .. } => {
                    entries.push(TranscriptEntry::new(EntryKind::Tool, name, input.to_string()));
                }
                ContentBlock::ToolResult { content, is_error, .. } => {
                    let kind = if *is_error { EntryKind::Error } else { EntryKind::Tool };
                    entries.push(TranscriptEntry::new(kind, "Result", content));
                }
                ContentBlock::Image { .. } => {
                    entries.push(TranscriptEntry::new(EntryKind::Info, "Image", "[attached image]"));
                }
                ContentBlock::ProviderItem { .. } => {}
            }
        }
    }
    entries
}
