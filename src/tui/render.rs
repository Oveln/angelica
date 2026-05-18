use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};

use super::constants::{
    ASSISTANT_GLYPH, CARD_BOT, CARD_MID, CARD_TOP, COLOR_ASSISTANT, COLOR_INPUT, COLOR_RAIL,
    COLOR_SYSTEM, COLOR_THINKING, COLOR_TOOL, COLOR_USER, RAIL, THINKING_RAIL, TOOL_GLYPH,
    USER_GLYPH,
};
use super::diff::render_diff_lines;
use super::ui::AppState;

// ── Line builder (borrows state immutably, runs before scroll mutation) ──

pub(super) fn build_all_lines(state: &AppState, terminal_width: usize) -> Text<'static> {
    let mut lines: Vec<Line> = Vec::new();

    for msg in state.filtered_messages() {
        match msg.role.as_str() {
            "user" => {
                render_user_message(&mut lines, &msg.content);
            }
            "assistant" => {
                render_assistant_message(&mut lines, state, &msg.content, msg.thinking.as_deref());
            }
            "tool" => {
                render_tool_message(&mut lines, &msg.content);
            }
            "system" => {
                render_system_message(&mut lines, &msg.content, msg.collapsed);
            }
            "diff" => {
                let diff_lines = render_diff_lines(&msg.content, terminal_width);
                lines.extend(diff_lines);
            }
            _ => {
                lines.push(Line::from(msg.content.clone()));
            }
        }
    }

    // Live streaming buffers
    if !state.thinking_buffer.is_empty() && state.thinking_visible {
        let glyph_style = Style::default()
            .fg(COLOR_ASSISTANT)
            .add_modifier(Modifier::BOLD);
        lines.push(Line::from(vec![
            Span::styled(ASSISTANT_GLYPH.to_string(), glyph_style),
            Span::styled(
                " thinking\u{2026}",
                Style::default()
                    .fg(COLOR_THINKING)
                    .add_modifier(Modifier::ITALIC),
            ),
        ]));
        for think_line in state.thinking_buffer.lines() {
            lines.push(Line::from(Span::styled(
                format!("{}{}", THINKING_RAIL, think_line),
                Style::default().fg(COLOR_THINKING),
            )));
        }
    }

    if !state.text_buffer.is_empty() {
        if state.thinking_buffer.is_empty() {
            let glyph_style = Style::default()
                .fg(COLOR_ASSISTANT)
                .add_modifier(Modifier::BOLD);
            lines.push(Line::from(vec![
                Span::styled(ASSISTANT_GLYPH.to_string(), glyph_style),
                Span::raw(" "),
            ]));
        }
        let rail_style = Style::default().fg(COLOR_RAIL);
        for content_line in state.text_buffer.lines() {
            lines.push(Line::from(vec![
                Span::styled(RAIL.to_string(), rail_style),
                Span::styled(content_line.to_string(), Style::default().fg(COLOR_INPUT)),
            ]));
        }
    }

    lines.push(Line::from(""));
    Text::from(lines)
}

// ── Per-role renderers ──

fn render_user_message(lines: &mut Vec<Line>, content: &str) {
    let glyph_style = Style::default().fg(COLOR_USER).add_modifier(Modifier::BOLD);
    let rail_style = Style::default().fg(COLOR_RAIL);
    let body_style = Style::default().fg(COLOR_USER);

    for (i, line) in content.lines().enumerate() {
        if i == 0 {
            lines.push(Line::from(vec![
                Span::styled(USER_GLYPH.to_string(), glyph_style),
                Span::styled(format!(" {}", line), body_style),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::styled(RAIL.to_string(), rail_style),
                Span::styled(line.to_string(), body_style),
            ]));
        }
    }
    lines.push(Line::from(""));
}

fn render_assistant_message(
    lines: &mut Vec<Line>,
    state: &AppState,
    content: &str,
    thinking: Option<&str>,
) {
    let glyph_style = Style::default()
        .fg(COLOR_ASSISTANT)
        .add_modifier(Modifier::BOLD);
    let rail_style = Style::default().fg(COLOR_RAIL);
    let body_style = Style::default().fg(COLOR_INPUT);

    // Thinking section
    if let Some(think) = thinking {
        if state.thinking_visible && !think.is_empty() {
            lines.push(Line::from(vec![
                Span::styled(ASSISTANT_GLYPH.to_string(), glyph_style),
                Span::styled(
                    " thinking\u{2026}",
                    Style::default()
                        .fg(COLOR_THINKING)
                        .add_modifier(Modifier::ITALIC),
                ),
            ]));
            for think_line in think.lines() {
                lines.push(Line::from(Span::styled(
                    format!("{}{}", THINKING_RAIL, think_line),
                    Style::default().fg(COLOR_THINKING),
                )));
            }
        }
    }

    // Body
    for (i, line) in content.lines().enumerate() {
        if i == 0 {
            lines.push(Line::from(vec![
                Span::styled(ASSISTANT_GLYPH.to_string(), glyph_style),
                Span::styled(format!(" {}", line), body_style),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::styled(RAIL.to_string(), rail_style),
                Span::styled(line.to_string(), body_style),
            ]));
        }
    }
    lines.push(Line::from(""));
}

fn render_tool_message(lines: &mut Vec<Line>, content: &str) {
    let tool_lines: Vec<&str> = content.lines().collect();
    let rail_style = Style::default().fg(COLOR_TOOL);
    let body_style = Style::default().fg(COLOR_TOOL);

    for (i, line) in tool_lines.iter().enumerate() {
        let prefix = if tool_lines.len() == 1 {
            TOOL_GLYPH
        } else if i == 0 {
            CARD_TOP
        } else if i == tool_lines.len() - 1 {
            CARD_BOT
        } else {
            CARD_MID
        };
        lines.push(Line::from(vec![
            Span::styled(prefix.to_string(), rail_style),
            Span::styled(line.to_string(), body_style),
        ]));
    }
}

fn render_system_message(lines: &mut Vec<Line>, content: &str, collapsed: bool) {
    let rail_style = Style::default().fg(COLOR_SYSTEM);
    let body_style = Style::default().fg(COLOR_SYSTEM);

    if collapsed {
        let all_lines: Vec<&str> = content.lines().collect();
        let (prefix, line) = if all_lines.len() == 1 {
            (TOOL_GLYPH, all_lines[0])
        } else {
            (CARD_TOP, all_lines[0])
        };
        lines.push(Line::from(vec![
            Span::styled(prefix.to_string(), rail_style),
            Span::styled(line.to_string(), body_style),
        ]));
        if all_lines.len() > 1 {
            lines.push(Line::from(vec![
                Span::styled(CARD_BOT.to_string(), rail_style),
                Span::styled(
                    format!("+{} lines [Ctrl+O]", all_lines.len().saturating_sub(1)),
                    Style::default().fg(COLOR_SYSTEM).add_modifier(Modifier::ITALIC),
                ),
            ]));
        }
    } else {
        let all_lines: Vec<&str> = content.lines().collect();
        for (i, line) in all_lines.iter().enumerate() {
            let prefix = if all_lines.len() == 1 {
                TOOL_GLYPH
            } else if i == 0 {
                CARD_TOP
            } else if i == all_lines.len() - 1 {
                CARD_BOT
            } else {
                CARD_MID
            };
            lines.push(Line::from(vec![
                Span::styled(prefix.to_string(), rail_style),
                Span::styled(line.to_string(), body_style),
            ]));
        }
    }
}
