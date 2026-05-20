use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use crate::tui::theme::{CARD_BOT, CARD_MID, CARD_TOP, Theme};

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

    lines.push(card_line(CARD_TOP, args_display, tool_style, width));

    if let Some(diff) = diff_preview {
        if collapsed {
            let added = diff.lines().filter(|l| l.starts_with('+') && !l.starts_with("+++")).count();
            let removed = diff.lines().filter(|l| l.starts_with('-') && !l.starts_with("---")).count();
            let summary = result.unwrap_or("");
            let rail_style = Style::default().fg(theme.rail).bg(panel_bg);
            lines.push(trailing_pad_line(
                CARD_BOT,
                vec![
                    Span::styled(
                        format!("+{}", added),
                        Style::default().fg(theme.success).bg(panel_bg).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        " ".to_string(),
                        Style::default().bg(panel_bg),
                    ),
                    Span::styled(
                        format!("-{}", removed),
                        Style::default().fg(theme.error).bg(panel_bg).add_modifier(Modifier::BOLD),
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
            // Skip render_diff_lines' own CARD_TOP/CARD_BOT — the card provides borders.
            let inner = if diff_lines.len() > 2 {
                &diff_lines[1..diff_lines.len() - 1]
            } else {
                &diff_lines[..]
            };
            for diff_line in inner {
                lines.push(diff_line.clone());
            }
            let summary = result.unwrap_or("");
            if summary.is_empty() {
                let rail_style = Style::default().fg(theme.rail).bg(panel_bg);
                lines.push(trailing_pad_line(
                    CARD_BOT,
                    vec![Span::styled(
                        String::new(),
                        Style::default().bg(panel_bg),
                    )],
                    rail_style,
                    width,
                ));
            } else {
                let rail_style = Style::default().fg(theme.rail).bg(panel_bg);
                lines.push(trailing_pad_line(
                    CARD_BOT,
                    vec![Span::styled(
                        summary.to_string(),
                        Style::default().fg(theme.muted).bg(panel_bg),
                    )],
                    rail_style,
                    width,
                ));
            }
        }
    } else if let Some(result) = result {
        let rail_style = Style::default().fg(theme.rail).bg(panel_bg);
        let body_style = Style::default().fg(theme.input).bg(panel_bg);
        if collapsed {
            let first_line = result.lines().next().unwrap_or("");
            let truncated: String = first_line.chars().take(80).collect();
            let extra = result.lines().count().saturating_sub(1);
            lines.push(trailing_pad_line(
                CARD_BOT,
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
                lines.push(trailing_pad_line(
                    prefix,
                    vec![Span::styled(line.to_string(), body_style)],
                    rail_style,
                    width,
                ));
            }
        }
    } else {
        let rail_style = Style::default().fg(theme.rail).bg(panel_bg);
        lines.push(trailing_pad_line(
            CARD_BOT,
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

fn card_line(prefix: &str, content: &str, style: Style, width: usize) -> Line<'static> {
    let prefix_w = UnicodeWidthStr::width(prefix);
    let content_w = UnicodeWidthStr::width(content);
    let pad_w = width.saturating_sub(prefix_w + content_w);
    Line::from(vec![
        Span::styled(prefix.to_string(), style),
        Span::styled(content.to_string(), style),
        Span::styled(" ".repeat(pad_w), style),
    ])
}

fn trailing_pad_line(
    prefix: &str,
    content_spans: Vec<Span<'static>>,
    rail_style: Style,
    width: usize,
) -> Line<'static> {
    let prefix_w = UnicodeWidthStr::width(prefix);
    let content_w: usize = content_spans
        .iter()
        .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
        .sum();
    let pad_w = width.saturating_sub(prefix_w + content_w);
    let mut spans = vec![Span::styled(prefix.to_string(), rail_style)];
    spans.extend(content_spans);
    spans.push(Span::styled(" ".repeat(pad_w), rail_style));
    Line::from(spans)
}
