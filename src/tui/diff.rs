use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use super::theme::{CARD_BOT, CARD_MID, CARD_TOP, Theme};

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

    lines.push(panel_line(CARD_TOP, "", panel_rail, max_width));

    for raw in preview.lines() {
        if raw == "\\ No newline at end of file" {
            continue;
        }

        if raw.starts_with("--- ") || raw.starts_with("+++ ") {
            lines.push(panel_line(
                CARD_MID,
                raw,
                Style::default()
                    .fg(theme.assistant)
                    .bg(panel_bg)
                    .add_modifier(Modifier::BOLD),
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
            lines.push(panel_line(
                CARD_MID,
                raw,
                Style::default().fg(theme.diff_hunk).bg(panel_bg),
                max_width,
            ));
            continue;
        }

        if raw.starts_with('+') && !raw.starts_with("+++") {
            let content = &raw[1..];
            let line_no = format_line_no(new_line);
            let gutter_prefix = format!(" {} ", line_no);
            let card_w = UnicodeWidthStr::width(CARD_MID);
            let gutter_w = UnicodeWidthStr::width(gutter_prefix.as_str()) + 2;
            let content_w = UnicodeWidthStr::width(content);
            let pad_w = max_width.saturating_sub(card_w + gutter_w + content_w);
            let bg = theme.diff_added_bg;
            let rail = Style::default().fg(theme.rail).bg(bg);
            lines.push(Line::from(vec![
                Span::styled(CARD_MID.to_string(), rail),
                Span::styled(gutter_prefix, Style::default().fg(theme.success).bg(bg)),
                Span::styled("+ ".to_string(), Style::default().fg(theme.success).bg(bg).add_modifier(Modifier::BOLD)),
                Span::styled(content.to_string(), Style::default().fg(Color::White).bg(bg)),
                Span::styled(" ".repeat(pad_w), Style::default().bg(bg)),
            ]));
            if let Some(n) = new_line.as_mut() {
                *n += 1;
            }
            continue;
        }

        if raw.starts_with('-') && !raw.starts_with("---") {
            let content = &raw[1..];
            let line_no = format_line_no(old_line);
            let gutter_prefix = format!(" {} ", line_no);
            let card_w = UnicodeWidthStr::width(CARD_MID);
            let gutter_w = UnicodeWidthStr::width(gutter_prefix.as_str()) + 2;
            let content_w = UnicodeWidthStr::width(content);
            let pad_w = max_width.saturating_sub(card_w + gutter_w + content_w);
            let bg = theme.diff_removed_bg;
            let rail = Style::default().fg(theme.rail).bg(bg);
            lines.push(Line::from(vec![
                Span::styled(CARD_MID.to_string(), rail),
                Span::styled(gutter_prefix, Style::default().fg(theme.error).bg(bg)),
                Span::styled("- ".to_string(), Style::default().fg(theme.error).bg(bg).add_modifier(Modifier::BOLD)),
                Span::styled(content.to_string(), Style::default().fg(Color::White).bg(bg)),
                Span::styled(" ".repeat(pad_w), Style::default().bg(bg)),
            ]));
            if let Some(n) = old_line.as_mut() {
                *n += 1;
            }
            continue;
        }

        if raw.starts_with(' ') {
            let content = &raw[1..];
            let gutter_text = format!(" {}   ", format_line_no(new_line));
            let card_w = UnicodeWidthStr::width(CARD_MID);
            let gutter_w = UnicodeWidthStr::width(gutter_text.as_str());
            let content_w = UnicodeWidthStr::width(content);
            let pad_w = max_width.saturating_sub(card_w + gutter_w + content_w);
            lines.push(Line::from(vec![
                Span::styled(CARD_MID.to_string(), panel_rail),
                Span::styled(gutter_text, Style::default().fg(theme.rail).bg(panel_bg)),
                Span::styled(content.to_string(), Style::default().fg(theme.input).bg(panel_bg)),
                Span::styled(" ".repeat(pad_w), panel_rail),
            ]));
            if let Some(n) = old_line.as_mut() {
                *n += 1;
            }
            if let Some(n) = new_line.as_mut() {
                *n += 1;
            }
            continue;
        }

        lines.push(panel_line(
            CARD_MID,
            raw,
            Style::default().fg(theme.diff_hunk).bg(panel_bg),
            max_width,
        ));
    }

    lines.push(panel_line(CARD_BOT, "", panel_rail, max_width));
    lines
}

fn panel_line(prefix: &str, content: &str, style: Style, max_width: usize) -> Line<'static> {
    let prefix_w = UnicodeWidthStr::width(prefix);
    let content_w = UnicodeWidthStr::width(content);
    let pad_w = max_width.saturating_sub(prefix_w + content_w);
    Line::from(vec![
        Span::styled(prefix.to_string(), style),
        Span::styled(content.to_string(), style),
        Span::styled(" ".repeat(pad_w), style),
    ])
}

fn format_line_no(line: Option<usize>) -> String {
    line.map(|v| format!("{:>4}", v))
        .unwrap_or_else(|| "    ".to_string())
}
