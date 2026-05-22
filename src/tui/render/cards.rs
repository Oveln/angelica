//! Tool result card rendering.
//!
//! Cards provide visually distinct panels for tool call results.
//! Design: minimal borders, consistent background, content-first layout.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::tui::theme::{CARD_BOT, CARD_MID, Theme};

use super::components::{card_bot, card_line, card_top};

pub(super) fn render_inline_tool(
    lines: &mut Vec<Line>,
    args_display: &str,
    result: Option<&str>,
    diff_preview: Option<&str>,
    collapsed: bool,
    theme: &Theme,
    width: usize,
) {
    let panel_bg = theme.panel_bg;
    let tool_style = Style::default().fg(theme.tool).bg(panel_bg);

    lines.push(card_top(args_display, tool_style, width));

    if let Some(diff) = diff_preview {
        render_diff_card(lines, diff, result, collapsed, theme, width, panel_bg);
    } else if let Some(result) = result {
        render_result_card(lines, result, collapsed, theme, width, panel_bg);
    } else {
        let rail_style = Style::default().fg(theme.rail).bg(panel_bg);
        lines.push(card_bot(
            vec![Span::styled(
                "running\u{2026}".to_string(),
                Style::default()
                    .fg(theme.muted)
                    .bg(panel_bg)
                    .add_modifier(Modifier::ITALIC),
            )],
            rail_style,
            width,
        ));
    }
}

fn render_diff_card(
    lines: &mut Vec<Line>,
    diff: &str,
    result: Option<&str>,
    collapsed: bool,
    theme: &Theme,
    width: usize,
    panel_bg: ratatui::style::Color,
) {
    if collapsed {
        let added = diff
            .lines()
            .filter(|l| l.starts_with('+') && !l.starts_with("+++"))
            .count();
        let removed = diff
            .lines()
            .filter(|l| l.starts_with('-') && !l.starts_with("---"))
            .count();
        let summary = result.unwrap_or("");
        let rail_style = Style::default().fg(theme.rail).bg(panel_bg);
        lines.push(card_bot(
            vec![
                Span::styled(
                    format!("+{}", added),
                    Style::default()
                        .fg(theme.success)
                        .bg(panel_bg)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" ".to_string(), Style::default().bg(panel_bg)),
                Span::styled(
                    format!("-{}", removed),
                    Style::default()
                        .fg(theme.error)
                        .bg(panel_bg)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("  {}", summary),
                    Style::default().fg(theme.muted).bg(panel_bg),
                ),
            ],
            rail_style,
            width,
        ));
    } else {
        let diff_lines = super::super::diff::render_diff_lines(diff, width, theme);
        let inner = if diff_lines.len() > 2 {
            &diff_lines[1..diff_lines.len() - 1]
        } else {
            &diff_lines[..]
        };
        for diff_line in inner {
            lines.push(diff_line.clone());
        }
        let summary = result.unwrap_or("");
        let rail_style = Style::default().fg(theme.rail).bg(panel_bg);
        let summary_span = if summary.is_empty() {
            Span::styled(String::new(), Style::default().bg(panel_bg))
        } else {
            Span::styled(
                summary.to_string(),
                Style::default().fg(theme.muted).bg(panel_bg),
            )
        };
        lines.push(card_bot(vec![summary_span], rail_style, width));
    }
}

fn render_result_card(
    lines: &mut Vec<Line>,
    result: &str,
    collapsed: bool,
    theme: &Theme,
    width: usize,
    panel_bg: ratatui::style::Color,
) {
    let rail_style = Style::default().fg(theme.rail).bg(panel_bg);
    let body_style = Style::default().fg(theme.input).bg(panel_bg);

    if collapsed {
        let first_line = result.lines().next().unwrap_or("");
        let truncated: String = first_line.chars().take(80).collect();
        let extra = result.lines().count().saturating_sub(1);
        lines.push(card_bot(
            vec![
                Span::styled(truncated, Style::default().fg(theme.muted).bg(panel_bg)),
                Span::styled(
                    format!(" +{} lines", extra),
                    Style::default()
                        .fg(theme.muted)
                        .bg(panel_bg)
                        .add_modifier(Modifier::ITALIC),
                ),
            ],
            rail_style,
            width,
        ));
    } else {
        let all_lines: Vec<&str> = result.lines().collect();
        for (i, line) in all_lines.iter().enumerate() {
            let prefix = if i == all_lines.len() - 1 {
                CARD_BOT
            } else {
                CARD_MID
            };
            lines.push(card_line(
                prefix,
                vec![Span::styled(line.to_string(), body_style)],
                rail_style,
                width,
            ));
        }
    }
}
