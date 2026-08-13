use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

#[derive(Debug, Clone, Copy)]
pub(super) struct MarkdownTheme {
    pub(super) body: Style,
    pub(super) inline_code: Style,
    pub(super) code_block: Style,
    pub(super) marker: Style,
    pub(super) rule: Style,
}

pub(super) fn render_markdown(input: &str, width: u16, theme: MarkdownTheme) -> Vec<Line<'static>> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    let parser = Parser::new_ext(input, options);
    let mut renderer = MarkdownRenderer::new(width.max(1), theme);
    renderer.render(parser);
    renderer.finish()
}

#[derive(Debug)]
struct CodeBlock {
    content: String,
}

#[derive(Debug)]
struct ListState {
    next: Option<u64>,
    marker: Option<String>,
    indent: usize,
}

impl ListState {
    fn new(next: Option<u64>) -> Self {
        Self {
            next,
            marker: None,
            indent: 2,
        }
    }

    fn begin_item(&mut self) {
        let marker = if let Some(next) = &mut self.next {
            let marker = format!("{next}. ");
            *next = next.saturating_add(1);
            marker
        } else {
            "• ".to_string()
        };
        self.indent = UnicodeWidthStr::width(marker.as_str());
        self.marker = Some(marker);
    }
}

#[derive(Debug, Default)]
struct InlineState {
    strong: usize,
    emphasis: usize,
    strikethrough: usize,
    link: usize,
    heading: Option<HeadingLevel>,
}

impl InlineState {
    fn style(&self, base: Style) -> Style {
        let mut modifiers = Modifier::empty();
        if self.strong > 0 || self.heading.is_some() {
            modifiers.insert(Modifier::BOLD);
        }
        if self.emphasis > 0 {
            modifiers.insert(Modifier::ITALIC);
        }
        if self.strikethrough > 0 {
            modifiers.insert(Modifier::CROSSED_OUT);
        }
        if self.link > 0 {
            modifiers.insert(Modifier::UNDERLINED);
        }
        if matches!(self.heading, Some(HeadingLevel::H1)) {
            modifiers.insert(Modifier::UNDERLINED);
        }
        base.add_modifier(modifiers)
    }
}

#[derive(Debug)]
struct LogicalLine {
    prefix: Vec<Span<'static>>,
    continuation_prefix: Vec<Span<'static>>,
    spans: Vec<Span<'static>>,
    fill: Option<Style>,
}

#[derive(Debug)]
struct MarkdownRenderer {
    width: u16,
    theme: MarkdownTheme,
    lines: Vec<Line<'static>>,
    current: Option<LogicalLine>,
    inline: InlineState,
    lists: Vec<ListState>,
    quote_depth: usize,
    code_block: Option<CodeBlock>,
    pending_separator: bool,
}

impl MarkdownRenderer {
    fn new(width: u16, theme: MarkdownTheme) -> Self {
        Self {
            width,
            theme,
            lines: Vec::new(),
            current: None,
            inline: InlineState::default(),
            lists: Vec::new(),
            quote_depth: 0,
            code_block: None,
            pending_separator: false,
        }
    }

    fn render<'a>(&mut self, parser: impl Iterator<Item = Event<'a>>) {
        for event in parser {
            if let Some(code_block) = self.code_block.as_mut() {
                match event {
                    Event::Text(text) | Event::Code(text) => code_block.content.push_str(&text),
                    Event::SoftBreak | Event::HardBreak => code_block.content.push('\n'),
                    Event::End(TagEnd::CodeBlock) => self.finish_code_block(),
                    _ => {}
                }
                continue;
            }

            match event {
                Event::Start(tag) => self.start_tag(tag),
                Event::End(tag) => self.end_tag(tag),
                Event::Text(text) | Event::Html(text) | Event::InlineHtml(text) => self.push_text(&text),
                Event::Code(code) => {
                    let style = self.inline.style(self.theme.inline_code);
                    self.push_span(Span::styled(code.to_string(), style));
                }
                Event::SoftBreak => self.push_soft_break(),
                Event::HardBreak => self.flush_current(),
                Event::Rule => self.push_rule(),
                Event::TaskListMarker(checked) => {
                    self.push_text(if checked { "[x] " } else { "[ ] " });
                }
                Event::FootnoteReference(label) => self.push_text(&format!("[{label}]")),
                Event::InlineMath(math) => self.push_text(&format!("${math}$")),
                Event::DisplayMath(math) => {
                    self.start_block();
                    self.push_text(&math);
                    self.flush_current();
                    self.pending_separator = true;
                }
            }
        }
    }

    fn start_tag(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => self.start_block(),
            Tag::Heading { level, .. } => {
                self.start_block();
                self.inline.heading = Some(level);
            }
            Tag::BlockQuote(_) => {
                self.start_block();
                self.quote_depth = self.quote_depth.saturating_add(1);
            }
            Tag::CodeBlock(_) => {
                self.start_block();
                self.code_block = Some(CodeBlock { content: String::new() });
            }
            Tag::List(next) => {
                if self.lists.is_empty() {
                    self.start_block();
                } else {
                    self.flush_current();
                }
                self.lists.push(ListState::new(next));
            }
            Tag::Item => {
                self.flush_current();
                if let Some(list) = self.lists.last_mut() {
                    list.begin_item();
                }
            }
            Tag::Emphasis => self.inline.emphasis = self.inline.emphasis.saturating_add(1),
            Tag::Strong => self.inline.strong = self.inline.strong.saturating_add(1),
            Tag::Strikethrough => self.inline.strikethrough = self.inline.strikethrough.saturating_add(1),
            Tag::Link { .. } => self.inline.link = self.inline.link.saturating_add(1),
            Tag::Image { .. } => self.inline.emphasis = self.inline.emphasis.saturating_add(1),
            Tag::HtmlBlock => self.start_block(),
            _ => {}
        }
    }

    fn end_tag(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => {
                self.flush_current();
                if self.lists.is_empty() {
                    self.pending_separator = true;
                }
            }
            TagEnd::Heading(_) => {
                self.flush_current();
                self.inline.heading = None;
                self.pending_separator = true;
            }
            TagEnd::BlockQuote(_) => {
                self.flush_current();
                self.quote_depth = self.quote_depth.saturating_sub(1);
                self.pending_separator = true;
            }
            TagEnd::CodeBlock => self.finish_code_block(),
            TagEnd::List(_) => {
                self.flush_current();
                self.lists.pop();
                if self.lists.is_empty() {
                    self.pending_separator = true;
                }
            }
            TagEnd::Item => {
                self.flush_current();
                if let Some(list) = self.lists.last_mut() {
                    list.marker = None;
                }
            }
            TagEnd::Emphasis => self.inline.emphasis = self.inline.emphasis.saturating_sub(1),
            TagEnd::Strong => self.inline.strong = self.inline.strong.saturating_sub(1),
            TagEnd::Strikethrough => self.inline.strikethrough = self.inline.strikethrough.saturating_sub(1),
            TagEnd::Link => self.inline.link = self.inline.link.saturating_sub(1),
            TagEnd::Image => self.inline.emphasis = self.inline.emphasis.saturating_sub(1),
            TagEnd::HtmlBlock => {
                self.flush_current();
                self.pending_separator = true;
            }
            _ => {}
        }
    }

    fn start_block(&mut self) {
        self.flush_current();
        if self.pending_separator && self.lines.last().is_some_and(|line| !line.spans.is_empty()) {
            self.lines.push(Line::default());
        }
        self.pending_separator = false;
    }

    fn ensure_current(&mut self) -> &mut LogicalLine {
        if self.current.is_none() {
            let (prefix, continuation_prefix) = self.take_prefixes();
            self.current = Some(LogicalLine {
                prefix,
                continuation_prefix,
                spans: Vec::new(),
                fill: None,
            });
        }
        self.current.as_mut().expect("current markdown line is initialized")
    }

    fn take_prefixes(&mut self) -> (Vec<Span<'static>>, Vec<Span<'static>>) {
        let mut prefix = Vec::new();
        let mut continuation = Vec::new();
        for _ in 0..self.quote_depth {
            prefix.push(Span::styled("│ ", self.theme.marker));
            continuation.push(Span::styled("│ ", self.theme.marker));
        }
        let list_count = self.lists.len();
        for (index, list) in self.lists.iter_mut().enumerate() {
            if index + 1 == list_count
                && let Some(marker) = list.marker.take()
            {
                continuation.push(Span::raw(" ".repeat(list.indent)));
                prefix.push(Span::styled(marker, self.theme.marker));
            } else {
                let indent = " ".repeat(list.indent);
                prefix.push(Span::raw(indent.clone()));
                continuation.push(Span::raw(indent));
            }
        }
        (prefix, continuation)
    }

    fn push_text(&mut self, text: &str) {
        let style = self.inline.style(self.theme.body);
        let mut parts = text.split('\n').peekable();
        while let Some(part) = parts.next() {
            if !part.is_empty() {
                self.push_span(Span::styled(part.to_string(), style));
            }
            if parts.peek().is_some() {
                self.flush_current();
            }
        }
    }

    fn push_span(&mut self, span: Span<'static>) {
        self.ensure_current().spans.push(span);
    }

    fn push_soft_break(&mut self) {
        let needs_space = self
            .current
            .as_ref()
            .and_then(|line| line.spans.last())
            .is_some_and(|span| {
                span.content
                    .chars()
                    .last()
                    .is_some_and(|character| !character.is_whitespace())
            });
        if needs_space {
            self.push_text(" ");
        }
    }

    fn push_rule(&mut self) {
        self.start_block();
        let width = usize::from(self.width).min(32);
        self.lines
            .push(Line::from(Span::styled("─".repeat(width), self.theme.rule)));
        self.pending_separator = true;
    }

    fn finish_code_block(&mut self) {
        let Some(code_block) = self.code_block.take() else {
            return;
        };
        let content = code_block.content.strip_suffix('\n').unwrap_or(&code_block.content);
        for code_line in content.split('\n') {
            let (mut prefix, mut continuation_prefix) = self.take_prefixes();
            prefix.push(Span::styled(" ", self.theme.code_block));
            continuation_prefix.push(Span::styled(" ", self.theme.code_block));
            self.push_logical(LogicalLine {
                prefix,
                continuation_prefix,
                spans: vec![Span::styled(code_line.to_string(), self.theme.code_block)],
                fill: Some(self.theme.code_block),
            });
        }
        self.pending_separator = true;
    }

    fn flush_current(&mut self) {
        if let Some(line) = self.current.take() {
            self.push_logical(line);
        }
    }

    fn push_logical(&mut self, line: LogicalLine) {
        self.lines.extend(wrap_line(line, self.width));
    }

    fn finish(mut self) -> Vec<Line<'static>> {
        if self.code_block.is_some() {
            self.finish_code_block();
        }
        self.flush_current();
        while self.lines.last().is_some_and(|line| line.spans.is_empty()) {
            self.lines.pop();
        }
        self.lines
    }
}

fn wrap_line(line: LogicalLine, width: u16) -> Vec<Line<'static>> {
    let max_width = usize::from(width.max(1));
    let initial_prefix_width = spans_width(&line.prefix);
    let continuation_prefix_width = spans_width(&line.continuation_prefix);
    let mut output = Vec::new();
    let mut row = line.prefix;
    let mut row_width = initial_prefix_width;
    let mut continuation = false;

    for span in line.spans {
        let style = span.style;
        let mut fragment = String::new();
        for character in span.content.chars() {
            let character_width = character.width().unwrap_or(1).max(1);
            let content_start = if continuation {
                continuation_prefix_width
            } else {
                initial_prefix_width
            };
            if row_width > content_start && row_width.saturating_add(character_width) > max_width {
                push_fragment(&mut row, &mut fragment, style);
                finish_row(&mut output, row, row_width, max_width, line.fill);
                row = line.continuation_prefix.clone();
                row_width = spans_width(&row);
                continuation = true;
            }
            fragment.push(character);
            row_width = row_width.saturating_add(character_width);
        }
        push_fragment(&mut row, &mut fragment, style);
    }

    finish_row(&mut output, row, row_width, max_width, line.fill);
    output
}

fn push_fragment(row: &mut Vec<Span<'static>>, fragment: &mut String, style: Style) {
    if !fragment.is_empty() {
        row.push(Span::styled(std::mem::take(fragment), style));
    }
}

fn finish_row(
    output: &mut Vec<Line<'static>>,
    mut row: Vec<Span<'static>>,
    row_width: usize,
    max_width: usize,
    fill: Option<Style>,
) {
    if let Some(fill) = fill
        && row_width < max_width
    {
        row.push(Span::styled(" ".repeat(max_width - row_width), fill));
    }
    output.push(Line::from(row));
}

fn spans_width(spans: &[Span<'_>]) -> usize {
    spans.iter().map(|span| span.width()).sum()
}

#[cfg(test)]
#[path = "markdown_test.rs"]
mod markdown_test;
