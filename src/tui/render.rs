use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};

use super::theme::{
    ASSISTANT_GLYPH, CARD_BOT, CARD_MID, CARD_TOP, RAIL, THINKING_RAIL, TOOL_GLYPH, USER_GLYPH,
};
use super::ui::AppState;

pub(super) fn build_all_lines(state: &AppState, terminal_width: usize) -> Text<'static> {
    let theme = state.theme();
    let mut lines: Vec<Line> = Vec::new();

    for msg in state.filtered_messages() {
        match msg.role.as_str() {
            "user" => {
                render_user_message(&mut lines, &msg.content, theme);
            }
            "assistant" => {
                render_assistant_message(&mut lines, state, &msg.content, msg.thinking.as_deref());
            }
            "tool" => {
                render_tool_message(&mut lines, &msg.content, theme);
            }
            "system" => {
                render_system_message(&mut lines, &msg.content, msg.collapsed, theme);
            }
            "diff" => {
                let diff_lines =
                    super::diff::render_diff_lines(&msg.content, terminal_width, theme);
                lines.extend(diff_lines);
            }
            _ => {
                lines.push(Line::from(msg.content.clone()));
            }
        }
    }

    if !state.thinking_buffer.is_empty() && state.thinking_visible {
        let glyph_style = Style::default()
            .fg(theme.assistant)
            .add_modifier(Modifier::BOLD);
        lines.push(Line::from(vec![
            Span::styled(ASSISTANT_GLYPH.to_string(), glyph_style),
            Span::styled(
                " thinking\u{2026}",
                Style::default()
                    .fg(theme.thinking)
                    .add_modifier(Modifier::ITALIC),
            ),
        ]));
        for think_line in state.thinking_buffer.lines() {
            lines.push(Line::from(Span::styled(
                format!("{}{}", THINKING_RAIL, think_line),
                Style::default().fg(theme.thinking),
            )));
        }
    }

    if !state.text_buffer.is_empty() {
        if state.thinking_buffer.is_empty() {
            let glyph_style = Style::default()
                .fg(theme.assistant)
                .add_modifier(Modifier::BOLD);
            lines.push(Line::from(vec![
                Span::styled(ASSISTANT_GLYPH.to_string(), glyph_style),
                Span::raw(" "),
            ]));
        }
        let rail_style = Style::default().fg(theme.rail);
        for content_line in state.text_buffer.lines() {
            lines.push(Line::from(vec![
                Span::styled(RAIL.to_string(), rail_style),
                Span::styled(content_line.to_string(), Style::default().fg(theme.input)),
            ]));
        }
    }

    lines.push(Line::from(""));
    Text::from(lines)
}

fn render_user_message(lines: &mut Vec<Line>, content: &str, theme: &super::theme::Theme) {
    let glyph_style = Style::default().fg(theme.user).add_modifier(Modifier::BOLD);
    let rail_style = Style::default().fg(theme.rail);
    let body_style = Style::default().fg(theme.user);

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
    let theme = state.theme();
    let glyph_style = Style::default()
        .fg(theme.assistant)
        .add_modifier(Modifier::BOLD);
    let rail_style = Style::default().fg(theme.rail);
    let body_style = Style::default().fg(theme.input);

    if let Some(think) = thinking {
        if state.thinking_visible && !think.is_empty() {
            lines.push(Line::from(vec![
                Span::styled(ASSISTANT_GLYPH.to_string(), glyph_style),
                Span::styled(
                    " thinking\u{2026}",
                    Style::default()
                        .fg(theme.thinking)
                        .add_modifier(Modifier::ITALIC),
                ),
            ]));
            for think_line in think.lines() {
                lines.push(Line::from(Span::styled(
                    format!("{}{}", THINKING_RAIL, think_line),
                    Style::default().fg(theme.thinking),
                )));
            }
        }
    }

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

fn render_tool_message(lines: &mut Vec<Line>, content: &str, theme: &super::theme::Theme) {
    let tool_lines: Vec<&str> = content.lines().collect();
    let rail_style = Style::default().fg(theme.tool);
    let body_style = Style::default().fg(theme.tool);

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

fn render_system_message(
    lines: &mut Vec<Line>,
    content: &str,
    collapsed: bool,
    theme: &super::theme::Theme,
) {
    let rail_style = Style::default().fg(theme.system);
    let body_style = Style::default().fg(theme.system);

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
                    Style::default()
                        .fg(theme.system)
                        .add_modifier(Modifier::ITALIC),
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
