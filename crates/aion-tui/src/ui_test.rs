use aion_agent::commands::CommandSpec;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Color;

use super::{aion_mark_lines, entry_style, render};
use crate::state::AppState;
use crate::transcript::{EntryKind, TranscriptEntry};

#[test]
fn slash_command_popup_renders_an_unframed_selected_command() {
    let mut state = AppState::new(
        "model".to_string(),
        "provider".to_string(),
        "/workspace".to_string(),
        true,
    );
    state.set_commands(vec![CommandSpec {
        name: "help".to_string(),
        aliases: Vec::new(),
        description: "List commands".to_string(),
    }]);
    state.composer.insert_text("/");
    state.popup.update("/");

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| render(frame, &state))
        .expect("render should succeed");
    let rendered = terminal.backend().to_string();
    assert!(rendered.contains("/help"));
    assert!(!rendered.contains("Commands"));
    assert!(
        !['│', '┌', '┐', '└', '┘']
            .iter()
            .any(|character| rendered.contains(*character))
    );

    let command_row = rendered
        .lines()
        .position(|line| line.contains("/help"))
        .expect("selected command should be visible") as u16;
    let final_content_cell = terminal
        .backend()
        .buffer()
        .cell((78, command_row))
        .expect("selected command should fill the content width");
    assert!(final_content_cell.modifier.contains(ratatui::style::Modifier::REVERSED));
}

#[test]
fn conversation_is_not_wrapped_in_an_outer_border() {
    let state = AppState::new(
        "model".to_string(),
        "provider".to_string(),
        "/workspace".to_string(),
        true,
    );
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| render(frame, &state))
        .expect("render should succeed");

    let rendered = terminal.backend().to_string();
    assert!(
        !['│', '┌', '┐', '└', '┘']
            .iter()
            .any(|character| rendered.contains(*character))
    );
    assert!(rendered.contains("────"));
}

#[test]
fn runtime_metadata_is_rendered_in_the_footer_not_the_top_row() {
    let mut state = AppState::new(
        "gpt-5.5".to_string(),
        "openai".to_string(),
        "/workspace/project".to_string(),
        true,
    );
    state.session_id = Some("session-id".to_string());
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| render(frame, &state))
        .expect("render should succeed");

    let lines = terminal.backend().to_string();
    let lines = lines.lines().collect::<Vec<_>>();
    assert!(!lines[0].contains("openai"));
    assert!(lines[22].contains("openai"));
    assert!(lines[23].contains("session session-id"));
}

#[test]
fn welcome_renders_ascii_aion_mark() {
    let state = AppState::new(
        "model".to_string(),
        "provider".to_string(),
        "/workspace".to_string(),
        true,
    );
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| render(frame, &state))
        .expect("render should succeed");

    let rendered = terminal.backend().to_string();
    assert!(rendered.contains("**@@@@**"));
    assert!(rendered.contains("**@@@@######@@**"));
    assert!(rendered.contains("AionCLI"));
}

#[test]
fn welcome_ascii_uses_truecolor_grayscale_without_a_background() {
    let state = AppState::new(
        "model".to_string(),
        "provider".to_string(),
        "/workspace".to_string(),
        false,
    );
    let mark = aion_mark_lines(&state);
    let colored = mark
        .iter()
        .flat_map(|line| &line.spans)
        .find(|span| span.content.contains('@'))
        .expect("mark should contain a dense colored cell");
    assert_eq!(colored.style.fg, Some(Color::Rgb(28, 28, 28)));
    assert_eq!(colored.style.bg, None);
}

#[test]
fn user_and_assistant_messages_have_distinct_backgrounds() {
    let state = AppState::new(
        "model".to_string(),
        "provider".to_string(),
        "/workspace".to_string(),
        false,
    );
    let user = entry_style(EntryKind::User, &state);
    let assistant = entry_style(EntryKind::Assistant, &state);
    assert_eq!(user.bg, Some(Color::Black));
    assert_eq!(assistant.bg, None);
}

#[test]
fn message_background_fills_the_current_render_width() {
    for width in [40, 64] {
        let mut state = AppState::new(
            "model".to_string(),
            "provider".to_string(),
            "/workspace".to_string(),
            false,
        );
        state
            .transcript
            .push(TranscriptEntry::new(EntryKind::User, "", "short message"));
        let backend = TestBackend::new(width, 16);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        terminal
            .draw(|frame| render(frame, &state))
            .expect("render should succeed");

        let rendered = terminal.backend().to_string();
        let message_row = rendered
            .lines()
            .position(|line| line.contains("short message"))
            .expect("message should be visible") as u16;
        let final_content_column = width - 2;
        let cell = terminal
            .backend()
            .buffer()
            .cell((final_content_column, message_row))
            .expect("last message cell should exist");
        assert_eq!(cell.bg, Color::Black);
    }
}

#[test]
fn composer_and_footer_have_a_blank_row_between_them() {
    let state = AppState::new(
        "model".to_string(),
        "provider".to_string(),
        "/workspace".to_string(),
        true,
    );
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| render(frame, &state))
        .expect("render should succeed");

    let rendered = terminal.backend().to_string();
    let lines: Vec<_> = rendered.lines().collect();
    let composer_row = lines
        .iter()
        .position(|line| line.contains("Type a message"))
        .expect("composer should be visible");
    let footer_row = lines
        .iter()
        .position(|line| line.contains("Enter send"))
        .expect("footer should be visible");
    assert!(footer_row >= composer_row + 2);
}

#[test]
fn undersized_terminal_shows_resize_message() {
    let state = AppState::new(
        "model".to_string(),
        "provider".to_string(),
        "/workspace".to_string(),
        true,
    );
    let backend = TestBackend::new(21, 7);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| render(frame, &state))
        .expect("render should succeed");
    assert!(terminal.backend().to_string().contains("Terminal too small"));
}
