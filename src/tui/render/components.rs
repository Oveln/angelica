//! Reusable rendering primitives for consistent TUI layout.
//!
//! Design principles:
//! - **Gestalt proximity**: spacing reflects semantic grouping
//! - **Visual hierarchy**: important content is prominent, metadata is subtle
//! - **Whitespace**: breathing room between sections, not within
//! - **Consistency**: same patterns look the same everywhere

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use crate::tui::theme::{CARD_BOT, CARD_MID, CARD_TOP, RAIL, Theme};

// ─── Spacing ─────────────────────────────────────────

/// A blank line for visual separation between messages.
pub fn spacer() -> Line<'static> {
    Line::from("")
}

/// A subtle half-height separator — a thin line of the rail color,
/// indented to align with content. Used between related items
/// within the same turn (e.g. between assistant text and tool calls).
#[allow(dead_code)]
pub fn thin_separator(theme: &Theme, width: usize) -> Line<'static> {
    let indent = UnicodeWidthStr::width(RAIL);
    let fill = width.saturating_sub(indent);
    Line::from(vec![
        Span::styled(RAIL.to_string(), Style::default().fg(theme.rail)),
        Span::styled(
            "\u{2500}".repeat(fill.min(40)),
            Style::default().fg(theme.border),
        ),
    ])
}

// ─── Card Lines ───────────────────────────────────────

/// Build a card line with prefix glyph, content spans, and right padding.
/// This is the fundamental building block for all card-style rendering
/// (tool results, diff blocks, system messages).
pub fn card_line(
    prefix: &str,
    content_spans: Vec<Span<'static>>,
    rail_style: Style,
    total_width: usize,
) -> Line<'static> {
    let prefix_w = UnicodeWidthStr::width(prefix);
    let content_w: usize = content_spans
        .iter()
        .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
        .sum();
    let pad_w = total_width.saturating_sub(prefix_w + content_w);
    let mut spans = vec![Span::styled(prefix.to_string(), rail_style)];
    spans.extend(content_spans);
    spans.push(Span::styled(" ".repeat(pad_w), rail_style));
    Line::from(spans)
}

/// Shorthand: card top border with single styled content string.
pub fn card_top(content: &str, style: Style, width: usize) -> Line<'static> {
    card_line(
        CARD_TOP,
        vec![Span::styled(content.to_string(), style)],
        style,
        width,
    )
}

/// Shorthand: card mid line with arbitrary content spans.
pub fn card_mid(spans: Vec<Span<'static>>, rail_style: Style, width: usize) -> Line<'static> {
    card_line(CARD_MID, spans, rail_style, width)
}

/// Shorthand: card bottom border with arbitrary content spans.
pub fn card_bot(spans: Vec<Span<'static>>, rail_style: Style, width: usize) -> Line<'static> {
    card_line(CARD_BOT, spans, rail_style, width)
}

// ─── Glyph Lines ─────────────────────────────────────

/// Build wrapped text lines with a glyph prefix on the first line
/// and a rail prefix on continuation lines.
///
/// This pattern is used for user messages, assistant messages, and
/// any content that needs a visual "speaker" indicator.
pub fn glyph_lines(
    content: &str,
    glyph: &str,
    glyph_fg: Color,
    body_fg: Color,
    rail_fg: Color,
    max_w: usize,
) -> Vec<Line<'static>> {
    let glyph_style = Style::default().fg(glyph_fg).add_modifier(Modifier::BOLD);
    let rail_style = Style::default().fg(rail_fg);
    let body_style = Style::default().fg(body_fg);
    let glyph_prefix_w = UnicodeWidthStr::width(glyph) + 1; // glyph + space
    let rail_prefix_w = UnicodeWidthStr::width(RAIL);

    let mut result = Vec::new();
    for (i, line) in content.lines().enumerate() {
        let (prefix_w, prefix_span) = if i == 0 {
            (
                glyph_prefix_w,
                Some(Span::styled(glyph.to_string(), glyph_style)),
            )
        } else {
            (
                rail_prefix_w,
                Some(Span::styled(RAIL.to_string(), rail_style)),
            )
        };
        let content_w = max_w.saturating_sub(prefix_w);
        let wrapped = super::text::wrap_str(line, content_w);
        for (wi, chunk) in wrapped.iter().enumerate() {
            let mut spans = Vec::new();
            if wi == 0 {
                if let Some(p) = prefix_span.clone() {
                    spans.push(p);
                }
            } else {
                spans.push(Span::styled(RAIL.to_string(), rail_style));
            }
            spans.push(Span::styled(chunk.clone(), body_style));
            result.push(Line::from(spans));
        }
    }
    result
}

// ─── Metadata ────────────────────────────────────────

/// Build a compact, right-aligned usage indicator for response tokens.
/// Uses Unicode arrows for a small visual footprint:
///   ↓5.2k  ↑1.2k  ◎0.8k  ⚡85%
pub fn usage_line(usage: &crate::usage::UsageMetrics, theme: &Theme, width: usize) -> Line<'static> {
    let dim = Style::default().fg(theme.rail);
    let mut spans: Vec<Span<'static>> = Vec::new();

    // Output tokens (what the model wrote)
    spans.push(Span::styled(format!("↓{}", format_tokens(usage.completion_tokens)), dim));
    spans.push(Span::styled(" ", dim));

    // Input tokens (prompt sent to model)
    spans.push(Span::styled(format!("↑{}", format_tokens(usage.prompt_tokens)), dim));

    if usage.reasoning_tokens > 0 {
        spans.push(Span::styled(" ", dim));
        spans.push(Span::styled(format!("◎{}", format_tokens(usage.reasoning_tokens)), dim));
    }

    let cache_total = usage.cache_hit_tokens + usage.cache_miss_tokens;
    if cache_total > 0 {
        spans.push(Span::styled(" ", dim));
        spans.push(Span::styled(format!("⚡{:.0}%", usage.cache_hit_rate() * 100.0), dim));
    }

    let content_w: usize = spans.iter().map(|s| UnicodeWidthStr::width(s.content.as_ref())).sum();
    let rail_w = UnicodeWidthStr::width(RAIL);
    let pad = width.saturating_sub(rail_w + content_w);
    let mut line_spans = vec![Span::styled(RAIL.to_string(), Style::default().fg(theme.rail))];
    line_spans.push(Span::styled(" ".repeat(pad), dim));
    line_spans.extend(spans);
    Line::from(line_spans)
}

fn format_tokens(tokens: u64) -> String {
    if tokens >= 1000 {
        format!("{:.1}k", tokens as f64 / 1000.0)
    } else {
        format!("{}", tokens)
    }
}
