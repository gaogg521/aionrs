use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::state::AppState;
use crate::transcript::EntryKind;

const MAX_COMPOSER_HEIGHT: u16 = 8;
const MAX_POPUP_ITEMS: usize = 7;
const FOOTER_HEIGHT: u16 = 3;
const AION_MARK_WIDTH: usize = 32;
// Quantized from the Aion PNG after removing its black canvas. Each level expands to two
// terminal columns so the source aspect ratio remains recognizable in monospace cells.
const AION_MARK_LEVELS: &[&str] = &[
    "0000000440000000",
    "0000003553000000",
    "0000015555200000",
    "0000045225510000",
    "0000353003540000",
    "0003540000353000",
    "0002200120022000",
    "0000000550000000",
    "1410000220000041",
    "0452000000002540",
    "0025420000245300",
    "0001355444531000",
    "0000001221000000",
];

pub(super) fn render(frame: &mut Frame<'_>, state: &AppState) {
    let area = frame.area();
    let content = Rect::new(
        area.x.saturating_add(1),
        area.y,
        area.width.saturating_sub(2),
        area.height,
    );

    if content.width < 20 || content.height < 6 {
        frame.render_widget(
            Paragraph::new("Terminal too small\nResize to at least 22×8").style(error(state)),
            content,
        );
        return;
    }

    let composer_width = content.width.saturating_sub(4).max(1);
    let composer_height = state
        .composer
        .visual_height(composer_width, MAX_COMPOSER_HEIGHT)
        .saturating_add(1);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(composer_height),
            Constraint::Length(1),
            Constraint::Length(FOOTER_HEIGHT),
        ])
        .split(content);

    render_transcript(frame, chunks[0], state);
    render_composer(frame, chunks[1], state);
    render_footer(frame, chunks[3], state);

    if state.approval.is_some() {
        render_approval(frame, content, state);
    } else if state.popup.is_visible(&state.composer.text()) {
        render_command_popup(frame, content, chunks[1], state);
    }
}

fn compact_cwd(cwd: &str) -> &str {
    cwd.rsplit(['/', '\\']).find(|part| !part.is_empty()).unwrap_or(cwd)
}

fn render_transcript(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    if state.transcript.is_empty() {
        render_welcome(frame, area, state);
        return;
    }

    let mut lines = Vec::new();
    for (index, entry) in state.transcript.iter().enumerate() {
        if index > 0 {
            lines.push(Line::default());
        }
        if is_conversation(entry.kind) {
            for line in conversation_lines(&entry.text, area.width) {
                lines.push(Line::from(Span::styled(line, entry_style(entry.kind, state))));
            }
        } else {
            lines.push(Line::from(Span::styled(
                entry.label.as_str(),
                entry_style(entry.kind, state).add_modifier(Modifier::BOLD),
            )));
            for line in entry.text.lines() {
                lines.push(Line::from(Span::styled(
                    line.to_string(),
                    entry_style(entry.kind, state),
                )));
            }
            if entry.text.is_empty() {
                lines.push(Line::default());
            }
        }
    }

    let line_count = transcript_line_count(state, area.width.max(1));
    let paragraph = Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false });
    let bottom = line_count.saturating_sub(area.height as usize);
    let scroll = bottom.saturating_sub(state.scroll_back).min(u16::MAX as usize) as u16;
    frame.render_widget(paragraph.scroll((scroll, 0)), area);
}

fn render_welcome(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let welcome_height = AION_MARK_LEVELS.len() as u16 + 3;
    if area.height < welcome_height || area.width < 50 {
        frame.render_widget(
            Paragraph::new("Ask about this project, or type / to see commands.")
                .alignment(Alignment::Center)
                .style(muted(state)),
            area,
        );
        return;
    }

    let top_padding = area.height.saturating_sub(welcome_height) / 3;
    let mut lines = vec![Line::default(); top_padding as usize];
    lines.extend(aion_mark_lines(state));
    lines.push(Line::default());
    lines.push(Line::from(Span::styled(
        "AionCLI",
        normal(state).add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(Span::styled(
        "Ask about this project, or type / to see commands.",
        muted(state),
    )));
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), area);
}

fn aion_mark_lines(state: &AppState) -> Vec<Line<'static>> {
    AION_MARK_LEVELS
        .iter()
        .map(|levels| {
            let spans = levels
                .chars()
                .map(|level| {
                    let (glyph, shade) = match level {
                        '1' => ("..", 178),
                        '2' => ("::", 146),
                        '3' => ("**", 110),
                        '4' => ("##", 70),
                        '5' => ("@@", 28),
                        _ => ("  ", 0),
                    };
                    let style = if level == '0' || state.no_color {
                        Style::default()
                    } else {
                        Style::default().fg(Color::Rgb(shade, shade, shade))
                    };
                    Span::styled(glyph, style)
                })
                .collect::<Vec<_>>();
            debug_assert_eq!(levels.len() * 2, AION_MARK_WIDTH);
            Line::from(spans)
        })
        .collect()
}

fn transcript_line_count(state: &AppState, width: u16) -> usize {
    if state.transcript.is_empty() {
        return 1;
    }
    let width = width.max(1) as usize;
    let mut count = state.transcript.len().saturating_sub(1);
    for entry in &state.transcript {
        if is_conversation(entry.kind) {
            count += conversation_lines(&entry.text, width as u16).len();
        } else {
            count += wrapped_height(&entry.label, width);
            if entry.text.is_empty() {
                count += 1;
            } else {
                count += entry
                    .text
                    .lines()
                    .map(|line| wrapped_height(line, width))
                    .sum::<usize>();
            }
        }
    }
    count
}

fn is_conversation(kind: EntryKind) -> bool {
    matches!(kind, EntryKind::User | EntryKind::Assistant)
}

fn conversation_lines(text: &str, width: u16) -> Vec<String> {
    let line_width = usize::from(width.max(2));
    let content_width = line_width.saturating_sub(2).max(1);
    let mut lines = Vec::new();

    for source_line in text.split('\n') {
        let mut chunk = String::new();
        let mut chunk_width = 0usize;
        for character in source_line.chars() {
            let character_width = character.width().unwrap_or(1);
            if chunk_width > 0 && chunk_width + character_width > content_width {
                lines.push(padded_message_line(&chunk, chunk_width, content_width));
                chunk.clear();
                chunk_width = 0;
            }
            chunk.push(character);
            chunk_width += character_width;
        }
        if chunk_width > 0 || source_line.is_empty() {
            lines.push(padded_message_line(&chunk, chunk_width, content_width));
        }
    }
    lines
}

fn padded_message_line(text: &str, text_width: usize, content_width: usize) -> String {
    format!(" {text}{} ", " ".repeat(content_width.saturating_sub(text_width)))
}

fn wrapped_height(text: &str, width: usize) -> usize {
    UnicodeWidthStr::width(text).max(1).div_ceil(width)
}

fn render_composer(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let block = Block::default().borders(Borders::TOP).border_style(divider(state));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let text = state.composer.text();
    let placeholder = text.is_empty() && !state.busy;
    let displayed = if placeholder {
        "Type a message…".to_string()
    } else {
        text
    };
    let (cursor_column, cursor_row) = state.composer.visual_cursor(inner.width.max(1));
    let vertical_scroll = cursor_row.saturating_sub(inner.height.saturating_sub(1));
    let style = if placeholder { muted(state) } else { normal(state) };
    frame.render_widget(
        Paragraph::new(displayed)
            .style(style)
            .wrap(Wrap { trim: false })
            .scroll((vertical_scroll, 0)),
        inner,
    );
    if !state.busy && state.approval.is_none() {
        frame.set_cursor_position((
            inner.x.saturating_add(cursor_column),
            inner.y.saturating_add(cursor_row.saturating_sub(vertical_scroll)),
        ));
    }
}

fn render_footer(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let hints = if state.approval.is_some() {
        " y allow once · a always allow · n/esc deny "
    } else if state.busy {
        " Ctrl+C stop · PgUp/PgDn scroll "
    } else if area.width >= 70 {
        " Enter send · Shift+Enter newline · / commands · Tab complete · Ctrl+C quit "
    } else {
        " Enter send · / commands · Ctrl+C quit "
    };
    let status = if state.busy {
        ["·", "••", "•••", "••"]
            .get(state.spinner_frame)
            .copied()
            .unwrap_or("·")
    } else {
        "ready"
    };
    let metadata = format!(
        " AionCLI · {} · {} · {} · {} ",
        state.provider,
        state.model,
        status,
        compact_cwd(&state.cwd)
    );
    let session = format!(" session {} ", state.session_id.as_deref().unwrap_or("new"));
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(hints, accent(state))),
            Line::from(Span::styled(metadata, muted(state))),
            Line::from(Span::styled(session, muted(state))),
        ]),
        area,
    );
}

fn render_command_popup(frame: &mut Frame<'_>, bounds: Rect, composer: Rect, state: &AppState) {
    let matches = state.popup.matches();
    let visible = matches.len().min(MAX_POPUP_ITEMS);
    let height = visible as u16;
    let width = bounds.width;
    let x = bounds.x;
    let y = composer.y.saturating_sub(height);
    let area = Rect::new(x, y, width, height);
    let mut lines = Vec::with_capacity(visible);
    for (index, command) in matches.iter().take(visible).enumerate() {
        let selected = index == state.popup.selected();
        let marker = if selected { "›" } else { " " };
        let line = format!("{marker} /{:<12} {}", command.name, command.description);
        let line_width = UnicodeWidthStr::width(line.as_str());
        let line = format!("{line}{}", " ".repeat(width as usize - line_width.min(width as usize)));
        let style = if selected { selected_style(state) } else { normal(state) };
        lines.push(Line::from(Span::styled(line, style)));
    }
    frame.render_widget(Clear, area);
    frame.render_widget(Paragraph::new(lines), area);
}

fn render_approval(frame: &mut Frame<'_>, bounds: Rect, state: &AppState) {
    let Some(request) = &state.approval else {
        return;
    };
    let width = bounds.width.saturating_sub(4).clamp(20, 88);
    let height = bounds.height.saturating_sub(2).clamp(6, 12);
    let area = centered_rect(width, height, bounds);
    let description = if request.description.trim().is_empty() {
        "This tool needs your approval."
    } else {
        request.description.as_str()
    };
    let content = format!("{}\n\n{}\n\n{}", request.name, description, request.input);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(content).wrap(Wrap { trim: false }).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Tool approval ")
                .border_style(warning(state)),
        ),
        area,
    );
}

fn centered_rect(width: u16, height: u16, bounds: Rect) -> Rect {
    let x = bounds.x + bounds.width.saturating_sub(width) / 2;
    let y = bounds.y + bounds.height.saturating_sub(height) / 2;
    Rect::new(x, y, width.min(bounds.width), height.min(bounds.height))
}

fn entry_style(kind: EntryKind, state: &AppState) -> Style {
    match kind {
        EntryKind::User if state.no_color => normal(state).add_modifier(Modifier::BOLD),
        EntryKind::User => Style::default()
            .fg(Color::White)
            .bg(Color::Black)
            .add_modifier(Modifier::BOLD),
        EntryKind::Assistant if state.no_color => normal(state),
        EntryKind::Assistant => Style::default().fg(Color::DarkGray),
        EntryKind::Thinking => muted(state).add_modifier(Modifier::ITALIC),
        EntryKind::Tool => success(state),
        EntryKind::Info => muted(state),
        EntryKind::Error => error(state),
    }
}

fn normal(_state: &AppState) -> Style {
    Style::default()
}

fn accent(state: &AppState) -> Style {
    color(state, Color::Black)
}

fn success(state: &AppState) -> Style {
    color(state, Color::DarkGray)
}

fn warning(state: &AppState) -> Style {
    color(state, Color::Black).add_modifier(Modifier::BOLD)
}

fn error(state: &AppState) -> Style {
    color(state, Color::Black).add_modifier(Modifier::BOLD)
}

fn muted(state: &AppState) -> Style {
    color(state, Color::DarkGray)
}

fn selected_style(_state: &AppState) -> Style {
    Style::default().add_modifier(Modifier::REVERSED)
}

fn divider(state: &AppState) -> Style {
    color(state, Color::DarkGray)
}

fn color(state: &AppState, color: Color) -> Style {
    if state.no_color {
        Style::default()
    } else {
        Style::default().fg(color)
    }
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
