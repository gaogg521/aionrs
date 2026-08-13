use ratatui::style::{Color, Modifier, Style};
use unicode_width::UnicodeWidthStr;

use super::{MarkdownTheme, render_markdown};

fn theme() -> MarkdownTheme {
    MarkdownTheme {
        body: Style::default().fg(Color::Black),
        inline_code: Style::default().fg(Color::Black).bg(Color::Rgb(232, 232, 232)),
        code_block: Style::default().fg(Color::Black).bg(Color::Rgb(245, 245, 245)),
        marker: Style::default().fg(Color::Rgb(92, 92, 92)),
        rule: Style::default().fg(Color::Rgb(146, 146, 146)),
    }
}

#[test]
fn inline_markdown_removes_delimiters_and_preserves_semantic_styles() {
    let lines = render_markdown("Use `cargo check`, **carefully**, with *context*.", 80, theme());
    let rendered = lines.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n");
    assert_eq!(rendered, "Use cargo check, carefully, with context.");
    assert!(!rendered.contains('`'));
    assert!(!rendered.contains('*'));

    let spans = &lines[0].spans;
    let code = spans
        .iter()
        .find(|span| span.content.contains("cargo check"))
        .expect("inline code span");
    assert_eq!(code.style.bg, Some(Color::Rgb(232, 232, 232)));
    let strong = spans
        .iter()
        .find(|span| span.content == "carefully")
        .expect("strong span");
    assert!(strong.style.add_modifier.contains(Modifier::BOLD));
    let emphasis = spans
        .iter()
        .find(|span| span.content == "context")
        .expect("emphasis span");
    assert!(emphasis.style.add_modifier.contains(Modifier::ITALIC));
}

#[test]
fn fenced_code_block_hides_fences_and_fills_each_code_row() {
    let lines = render_markdown("```rust\nfn main() {\n    println!(\"hi\");\n}\n```", 28, theme());
    let rendered = lines.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n");
    assert!(!rendered.contains("rust"));
    assert!(rendered.contains("fn main()"));
    assert!(rendered.contains("    println!"));
    assert!(!rendered.contains("```"));

    for line in &lines {
        assert_eq!(UnicodeWidthStr::width(line.to_string().as_str()), 28);
        assert_eq!(
            line.spans.last().and_then(|span| span.style.bg),
            Some(Color::Rgb(245, 245, 245))
        );
    }
}

#[test]
fn lists_quotes_and_headings_render_without_source_markers() {
    let markdown = "## Summary\n\n- first\n- **second**\n\n> quoted `value`";
    let lines = render_markdown(markdown, 40, theme());
    let rendered = lines.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n");
    assert!(rendered.contains("Summary"));
    assert!(rendered.contains("• first"));
    assert!(rendered.contains("• second"));
    assert!(rendered.contains("│ quoted value"));
    assert!(!rendered.contains("##"));
    assert!(!rendered.contains("**"));

    let heading = lines
        .iter()
        .flat_map(|line| &line.spans)
        .find(|span| span.content == "Summary")
        .expect("heading span");
    assert!(heading.style.add_modifier.contains(Modifier::BOLD));
}

#[test]
fn wrapping_respects_cjk_width_and_keeps_code_background() {
    let lines = render_markdown("```\n中文中文中文中文\n```", 7, theme());
    assert_eq!(
        lines.len(),
        3,
        "rendered lines: {:?}",
        lines.iter().map(ToString::to_string).collect::<Vec<_>>()
    );
    for line in lines {
        assert_eq!(UnicodeWidthStr::width(line.to_string().as_str()), 7);
        assert_eq!(
            line.spans.last().and_then(|span| span.style.bg),
            Some(Color::Rgb(245, 245, 245))
        );
    }
}
