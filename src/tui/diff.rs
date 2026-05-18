use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use super::constants::{COLOR_ASSISTANT, COLOR_INPUT, COLOR_RAIL, COLOR_TOOL};

pub(super) fn render_diff_lines(preview: &str, max_width: usize) -> Vec<Line<'static>> {
    let mut lines: Vec<Line> = Vec::new();
    let mut old_line: Option<usize> = None;
    let mut new_line: Option<usize> = None;

    for raw in preview.lines() {
        if raw == "\\ No newline at end of file" {
            continue;
        }
        if raw.starts_with("--- ") || raw.starts_with("+++ ") {
            lines.push(Line::from(Span::styled(
                raw.to_string(),
                Style::default()
                    .fg(COLOR_ASSISTANT)
                    .add_modifier(Modifier::BOLD),
            )));
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
            lines.push(Line::from(Span::styled(
                raw.to_string(),
                Style::default().fg(COLOR_TOOL),
            )));
            continue;
        }

        if raw.starts_with('+') && !raw.starts_with("+++") {
            let content = &raw[1..];
            let gutter = format_line_no(None, new_line);
            let fill = max_width.saturating_sub(content.len());
            let padded = format!("{}{}", content, " ".repeat(fill));
            let bg = Color::Rgb(0, 40, 0);
            let fg = Color::Rgb(100, 255, 100);
            lines.push(Line::from(vec![
                Span::styled(format!("{gutter} + "), Style::default().fg(fg).bg(bg)),
                Span::styled(padded, Style::default().fg(Color::White).bg(bg)),
            ]));
            if let Some(n) = new_line.as_mut() {
                *n += 1;
            }
            continue;
        }

        if raw.starts_with('-') && !raw.starts_with("---") {
            let content = &raw[1..];
            let gutter = format_line_no(old_line, None);
            let fill = max_width.saturating_sub(content.len());
            let padded = format!("{}{}", content, " ".repeat(fill));
            let bg = Color::Rgb(40, 0, 0);
            let fg = Color::Rgb(255, 100, 100);
            lines.push(Line::from(vec![
                Span::styled(format!("{gutter} - "), Style::default().fg(fg).bg(bg)),
                Span::styled(padded, Style::default().fg(Color::White).bg(bg)),
            ]));
            if let Some(n) = old_line.as_mut() {
                *n += 1;
            }
            continue;
        }

        if raw.starts_with(' ') {
            let content = &raw[1..];
            let gutter = format_line_no(None, new_line);
            lines.push(Line::from(vec![
                Span::styled(format!("{gutter}   "), Style::default().fg(COLOR_RAIL)),
                Span::styled(content.to_string(), Style::default().fg(COLOR_INPUT)),
            ]));
            if let Some(n) = old_line.as_mut() {
                *n += 1;
            }
            if let Some(n) = new_line.as_mut() {
                *n += 1;
            }
            continue;
        }

        lines.push(Line::from(Span::styled(
            raw.to_string(),
            Style::default().fg(COLOR_TOOL),
        )));
    }

    lines
}

fn format_line_no(old: Option<usize>, new: Option<usize>) -> String {
    let old_str = old
        .map(|v| format!("{:>4}", v))
        .unwrap_or_else(|| "    ".to_string());
    let new_str = new
        .map(|v| format!("{:>4}", v))
        .unwrap_or_else(|| "    ".to_string());
    format!("{} {}", old_str, new_str)
}
