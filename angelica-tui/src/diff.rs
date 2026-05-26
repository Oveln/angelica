//! Diff rendering with line numbers and color-coded additions/removals.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use super::render::components::{card_bot, card_line, card_mid, card_top};
use super::theme::{CARD_MID, Theme};

pub(super) fn render_diff_lines(
    preview: &str,
    max_width: usize,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let mut lines: Vec<Line> = Vec::new();
    let mut old_line: Option<usize> = None;
    let mut new_line: Option<usize> = None;

    let panel_bg = theme.diff_context_bg;
    let panel_rail = Style::default().fg(theme.rail).bg(panel_bg);

    lines.push(card_top("", panel_rail, max_width));

    for raw in preview.lines() {
        if raw == "\\ No newline at end of file" {
            continue;
        }

        if raw.starts_with("--- ") || raw.starts_with("+++ ") {
            lines.push(card_mid(
                vec![Span::styled(
                    raw.to_string(),
                    Style::default()
                        .fg(theme.assistant)
                        .bg(panel_bg)
                        .add_modifier(Modifier::BOLD),
                )],
                panel_rail,
                max_width,
            ));
            continue;
        }

        if raw.starts_with("@@") {
            let parts: Vec<&str> = raw.split_whitespace().collect();
            if parts.len() >= 3 {
                let old_part = parts[1].trim_start_matches('-');
                let new_part = parts[2].trim_start_matches('+');
                old_line = old_part.split(',').next().and_then(|s| s.parse().ok());
                new_line = new_part.split(',').next().and_then(|s| s.parse().ok());
            }
            lines.push(card_mid(
                vec![Span::styled(
                    raw.to_string(),
                    Style::default().fg(theme.diff_hunk).bg(panel_bg),
                )],
                panel_rail,
                max_width,
            ));
            continue;
        }

        if raw.starts_with('+') && !raw.starts_with("+++") {
            let content = &raw[1..];
            let line_no = format_line_no(new_line);
            let gutter = format!(" {} + ", line_no);
            let bg = theme.diff_added_bg;
            let rail = Style::default().fg(theme.rail).bg(bg);
            let content_w = UnicodeWidthStr::width(content);
            let gutter_w = UnicodeWidthStr::width(gutter.as_str());
            let card_w = UnicodeWidthStr::width(CARD_MID);
            let pad_w = max_width.saturating_sub(card_w + gutter_w + content_w);
            lines.push(card_line(
                CARD_MID,
                vec![
                    Span::styled(gutter, Style::default().fg(theme.success).bg(bg)),
                    Span::styled(
                        content.to_string(),
                        Style::default().fg(Color::White).bg(bg),
                    ),
                    Span::styled(" ".repeat(pad_w), Style::default().bg(bg)),
                ],
                rail,
                max_width,
            ));
            if let Some(n) = new_line.as_mut() {
                *n += 1;
            }
            continue;
        }

        if raw.starts_with('-') && !raw.starts_with("---") {
            let content = &raw[1..];
            let line_no = format_line_no(old_line);
            let gutter = format!(" {} - ", line_no);
            let bg = theme.diff_removed_bg;
            let rail = Style::default().fg(theme.rail).bg(bg);
            let content_w = UnicodeWidthStr::width(content);
            let gutter_w = UnicodeWidthStr::width(gutter.as_str());
            let card_w = UnicodeWidthStr::width(CARD_MID);
            let pad_w = max_width.saturating_sub(card_w + gutter_w + content_w);
            lines.push(card_line(
                CARD_MID,
                vec![
                    Span::styled(gutter, Style::default().fg(theme.error).bg(bg)),
                    Span::styled(
                        content.to_string(),
                        Style::default().fg(Color::White).bg(bg),
                    ),
                    Span::styled(" ".repeat(pad_w), Style::default().bg(bg)),
                ],
                rail,
                max_width,
            ));
            if let Some(n) = old_line.as_mut() {
                *n += 1;
            }
            continue;
        }

        if let Some(content) = raw.strip_prefix(' ') {
            let gutter = format!(" {}   ", format_line_no(new_line));
            let content_w = UnicodeWidthStr::width(content);
            let gutter_w = UnicodeWidthStr::width(gutter.as_str());
            let card_w = UnicodeWidthStr::width(CARD_MID);
            let pad_w = max_width.saturating_sub(card_w + gutter_w + content_w);
            lines.push(card_line(
                CARD_MID,
                vec![
                    Span::styled(gutter, Style::default().fg(theme.rail).bg(panel_bg)),
                    Span::styled(
                        content.to_string(),
                        Style::default().fg(theme.input).bg(panel_bg),
                    ),
                    Span::styled(" ".repeat(pad_w), Style::default().bg(panel_bg)),
                ],
                panel_rail,
                max_width,
            ));
            if let Some(n) = old_line.as_mut() {
                *n += 1;
            }
            if let Some(n) = new_line.as_mut() {
                *n += 1;
            }
        }
    }

    lines.push(card_bot(
        vec![Span::styled(String::new(), Style::default().bg(panel_bg))],
        panel_rail,
        max_width,
    ));

    lines
}

fn format_line_no(line: Option<usize>) -> String {
    match line {
        Some(n) => format!("{:>4}", n),
        None => "    ".to_string(),
    }
}
