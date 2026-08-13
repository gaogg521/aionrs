use aion_agent::commands::CommandSpec;
use aion_types::message::{ContentBlock, Message, Role};

use super::AppState;
use crate::event::AgentEvent;
use crate::transcript::{EntryKind, ToolStepStatus, TranscriptEntry};

fn state() -> AppState {
    AppState::new(
        "test-model".to_string(),
        "test-provider".to_string(),
        ".".to_string(),
        true,
    )
}

#[test]
fn resumed_history_is_rendered_by_role() {
    let mut state = state();
    state.set_history(&[
        Message::new(
            Role::User,
            vec![ContentBlock::Text {
                text: "hello".to_string(),
            }],
        ),
        Message::new(Role::Assistant, vec![ContentBlock::Text { text: "hi".to_string() }]),
    ]);
    assert_eq!(state.transcript.len(), 2);
    assert_eq!(state.transcript[0].kind, EntryKind::User);
    assert_eq!(state.transcript[1].kind, EntryKind::Assistant);
    assert!(state.transcript[0].label.is_empty());
    assert!(state.transcript[1].label.is_empty());
}

#[test]
fn protocol_result_suppresses_duplicate_sink_result() {
    let mut state = state();
    state.handle_agent_event(AgentEvent::ProtocolToolResult {
        call_id: "call-1".to_string(),
        name: "Read".to_string(),
        is_error: false,
        content: "done".to_string(),
    });
    state.handle_agent_event(AgentEvent::ToolResult {
        call_id: "call-1".to_string(),
        name: "Read".to_string(),
        is_error: false,
        content: "done".to_string(),
    });
    assert_eq!(state.transcript.len(), 1);
}

#[test]
fn tool_lifecycle_updates_one_countable_step() {
    let mut state = state();
    state.handle_agent_event(AgentEvent::ToolCall {
        call_id: "call-1".to_string(),
        name: "Read".to_string(),
        input: "{\"path\":\"file.rs\"}".to_string(),
    });
    state.handle_agent_event(AgentEvent::ToolRunning {
        call_id: "call-1".to_string(),
        name: "Read".to_string(),
    });
    state.handle_agent_event(AgentEvent::ProtocolToolResult {
        call_id: "call-1".to_string(),
        name: "Read".to_string(),
        is_error: false,
        content: "file contents".to_string(),
    });

    assert_eq!(state.transcript.len(), 1);
    assert_eq!(state.transcript[0].label, "Read");
    assert_eq!(state.transcript[0].text, "file contents");
    assert_eq!(state.transcript[0].tool_status, Some(ToolStepStatus::Success));
}

#[test]
fn resumed_tool_use_and_result_merge_into_one_step() {
    let mut state = state();
    state.set_history(&[
        Message::new(
            Role::Assistant,
            vec![ContentBlock::ToolUse {
                id: "call-1".to_string(),
                name: "Read".to_string(),
                input: serde_json::json!({"path": "file.rs"}),
                extra: None,
            }],
        ),
        Message::new(
            Role::User,
            vec![ContentBlock::ToolResult {
                tool_use_id: "call-1".to_string(),
                content: "file contents".to_string(),
                is_error: false,
            }],
        ),
    ]);

    assert_eq!(state.transcript.len(), 1);
    assert_eq!(state.transcript[0].label, "Read");
    assert_eq!(state.transcript[0].text, "file contents");
    assert_eq!(state.transcript[0].tool_status, Some(ToolStepStatus::Success));
    assert!(!state.show_welcome);
}

#[test]
fn resumed_history_keeps_conversation_text_across_tool_rounds() {
    let mut state = state();
    state.set_history(&[
        Message::new(
            Role::User,
            vec![ContentBlock::Text {
                text: "first question".to_string(),
            }],
        ),
        Message::new(
            Role::Assistant,
            vec![ContentBlock::Text {
                text: "first answer".to_string(),
            }],
        ),
        Message::new(
            Role::User,
            vec![ContentBlock::Text {
                text: "second question".to_string(),
            }],
        ),
        Message::new(
            Role::Assistant,
            vec![ContentBlock::ToolUse {
                id: "call-1".to_string(),
                name: "Read".to_string(),
                input: serde_json::json!({"path": "file.rs"}),
                extra: None,
            }],
        ),
        Message::new(
            Role::User,
            vec![ContentBlock::ToolResult {
                tool_use_id: "call-1".to_string(),
                content: "file contents".to_string(),
                is_error: false,
            }],
        ),
        Message::new(
            Role::Assistant,
            vec![ContentBlock::Text {
                text: "complete final answer".to_string(),
            }],
        ),
    ]);

    let conversation = state
        .transcript
        .iter()
        .filter(|entry| matches!(entry.kind, EntryKind::User | EntryKind::Assistant))
        .map(|entry| entry.text.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        conversation,
        [
            "first question",
            "first answer",
            "second question",
            "complete final answer"
        ]
    );
    assert!(state.transcript.iter().any(|entry| {
        entry.kind == EntryKind::Tool
            && entry.label == "Read"
            && entry.text == "file contents"
            && entry.tool_status == Some(ToolStepStatus::Success)
    }));
}

#[test]
fn streaming_deltas_append_to_one_assistant_entry() {
    let mut state = state();
    state.handle_agent_event(AgentEvent::StreamStart);
    state.handle_agent_event(AgentEvent::TextDelta("hello ".to_string()));
    state.handle_agent_event(AgentEvent::TextDelta("world".to_string()));
    assert_eq!(state.transcript.len(), 1);
    assert_eq!(state.transcript[0].text, "hello world");
}

#[test]
fn command_catalog_merges_agent_and_tui_commands() {
    let mut state = state();
    state.set_commands(vec![CommandSpec {
        name: "compact".to_string(),
        aliases: Vec::new(),
        description: "Compress context".to_string(),
    }]);

    let names = state
        .popup
        .commands()
        .iter()
        .map(|command| command.name.as_str())
        .collect::<Vec<_>>();
    assert!(names.contains(&"compact"));
    assert!(names.contains(&"status"));
    assert!(names.contains(&"permissions"));
    assert!(names.contains(&"resume"));
}

#[test]
fn reset_session_clears_transient_state() {
    let mut state = state();
    state.composer.insert_text("draft");
    state.begin_initialization();
    state.busy = true;
    state.turns = 3;
    state
        .transcript
        .push(TranscriptEntry::new(EntryKind::Info, "Info", "old session"));

    state.reset_session(
        "next-model".to_string(),
        "next-provider".to_string(),
        Some("next-session".to_string()),
        &[],
    );

    assert_eq!(state.model, "next-model");
    assert_eq!(state.session_id.as_deref(), Some("next-session"));
    assert!(state.transcript.is_empty());
    assert!(state.composer.is_empty());
    assert!(!state.initializing);
    assert!(!state.busy);
    assert_eq!(state.turns, 0);
}
