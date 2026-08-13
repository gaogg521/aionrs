use aion_types::message::{ContentBlock, Message, Role};

use super::AppState;
use crate::event::AgentEvent;
use crate::transcript::EntryKind;

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
fn streaming_deltas_append_to_one_assistant_entry() {
    let mut state = state();
    state.handle_agent_event(AgentEvent::StreamStart);
    state.handle_agent_event(AgentEvent::TextDelta("hello ".to_string()));
    state.handle_agent_event(AgentEvent::TextDelta("world".to_string()));
    assert_eq!(state.transcript.len(), 1);
    assert_eq!(state.transcript[0].text, "hello world");
}
