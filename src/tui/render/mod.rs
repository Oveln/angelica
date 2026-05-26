//! Message rendering pipeline.
//!
//! Converts `DisplayMessage` list into `Text` lines with consistent visual hierarchy:
//! - Content (user/assistant messages) is prominent
//! - Tool cards provide structured panels
//! - Metadata (usage, status) is subtle and non-competitive

mod cards;
pub(super) mod components;
mod text;

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use unicode_width::UnicodeWidthStr;

use crate::llm::types::Role;

use super::state::AppState;
use super::theme::{
    ASSISTANT_GLYPH, CARD_BOT, CARD_MID, CARD_TOP, THINKING_RAIL, TOOL_GLYPH, USER_GLYPH,
};
use super::types::{ClickRange, DisplayMessage};

use cards::render_inline_tool;
use components::{glyph_lines, spacer, usage_line};
use text::{apply_line_selection, wrap_str};

const TOGGLE_LABEL: &str = "[toggle]";

pub(super) struct BuildResult {
    pub text: Text<'static>,
    pub click_ranges: Vec<ClickRange>,
    pub line_texts: Vec<String>,
}

pub(super) fn build_all_lines(state: &AppState, terminal_width: usize) -> BuildResult {
    let theme = state.theme();
    let mut lines: Vec<Line> = Vec::new();
    let mut click_ranges: Vec<ClickRange> = Vec::new();

    // Track the role of the previous message to control spacing.
    // Consecutive messages within the same "turn" (e.g. assistant + tool calls)
    // get less spacing than messages between different speakers.
    let mut prev_role: Option<Role> = None;

    for (msg_idx, msg) in state.messages.iter().enumerate() {
        if msg.is_hidden() {
            continue;
        }

        // Spacing: different speakers get a full blank line;
        // same-turn messages (assistant → tool) get a thin separator.
        let current_role = msg_role(msg);
        if !lines.is_empty() {
            match (prev_role, current_role) {
                // Tool following an assistant — same turn, thin separator
                (Some(Role::Assistant), Some(Role::Tool)) | (Some(Role::Tool), Some(Role::Tool)) => {
                    lines.push(spacer());
                }
                // Otherwise — full break between speakers
                _ => {
                    lines.push(spacer());
                }
            }
        }
        prev_role = current_role;
        let line_count_before = lines.len();

        match msg {
            DisplayMessage::Chat {
                role,
                content,
                thinking,
                collapsed,
                token_usage,
                ..
            } => match role {
                Role::User => {
                    lines.extend(glyph_lines(
                        content,
                        USER_GLYPH,
                        theme.user,
                        theme.user,
                        theme.rail,
                        terminal_width,
                    ));
                }
                Role::Assistant => {
                    render_assistant_message(
                        &mut lines,
                        state,
                        content,
                        thinking.as_deref(),
                        terminal_width,
                    );
                    if let Some(usage) = token_usage {
                        lines.push(usage_line(usage, theme, terminal_width));
                    }
                }
                Role::System => {
                    render_system_message(&mut lines, content, *collapsed, theme, terminal_width);
                }
                Role::Tool => {}
            },
            DisplayMessage::Tool {
                args_display,
                result,
                diff_preview,
                collapsed,
                ..
            } => {
                render_inline_tool(
                    &mut lines,
                    args_display,
                    result.as_deref(),
                    diff_preview.as_deref(),
                    *collapsed,
                    theme,
                    terminal_width,
                );
            }
            DisplayMessage::Diff { content, .. } => {
                let diff_lines = super::diff::render_diff_lines(content, terminal_width, theme);
                lines.extend(diff_lines);
            }
        }

        let can_toggle = match msg {
            DisplayMessage::Chat { collapsed, .. } => *collapsed,
            DisplayMessage::Tool { result, .. } => result.is_some(),
            _ => false,
        };

        if can_toggle && lines.len() > line_count_before {
            let toggle_line = lines.len() - 1;
            let is_hovered = state.viewport.hovered_msg_index == Some(msg_idx);
            let style = toggle_style(theme, is_hovered);
            let toggle_text = format!(" {TOGGLE_LABEL}");
            let toggle_w = UnicodeWidthStr::width(toggle_text.as_str());

            if let Some(line) = lines.get_mut(toggle_line) {
                let spans_before_pad = if line.spans.len() > 1 {
                    line.spans.len() - 1
                } else {
                    0
                };
                let content_w: usize = line.spans[..spans_before_pad]
                    .iter()
                    .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
                    .sum();
                let pad_needed = terminal_width.saturating_sub(content_w + toggle_w);

                if let Some(last) = line.spans.last_mut() {
                    last.content = " ".repeat(pad_needed).into();
                }
                let toggle_col = content_w + pad_needed;
                line.spans.push(Span::styled(toggle_text, style));
                click_ranges.push(ClickRange {
                    line: toggle_line,
                    col_start: toggle_col,
                    col_end: toggle_col + toggle_w,
                    msg_index: msg_idx,
                });
            }
        }
    }

    if !state.thinking_buffer.is_empty() && state.display.thinking_visible {
        if !lines.is_empty() {
            lines.push(spacer());
        }
        let think_style = Style::default().fg(theme.thinking);
        let think_w = terminal_width.saturating_sub(UnicodeWidthStr::width(THINKING_RAIL));
        for think_line in state.thinking_buffer.lines() {
            for chunk in wrap_str(think_line, think_w) {
                lines.push(Line::from(Span::styled(
                    format!("{}{}", THINKING_RAIL, chunk),
                    think_style,
                )));
            }
        }
    }

    if !state.text_buffer.is_empty() {
        if !lines.is_empty() && lines.last().is_none_or(|l| !l.spans.is_empty()) {
            lines.push(spacer());
        }
        lines.extend(glyph_lines(
            &state.text_buffer,
            ASSISTANT_GLYPH,
            theme.assistant,
            theme.input,
            theme.rail,
            terminal_width,
        ));
    }

    let sel = state.mouse.selection;
    if let Some((sl, sc, el, ec)) = sel {
        let (sel_start_line, sel_start_col, sel_end_line, sel_end_col) = if sl <= el {
            (sl, sc, el, ec)
        } else {
            (el, ec, sl, sc)
        };

        for line_idx in sel_start_line..=sel_end_line {
            let sel_col_start = if line_idx == sel_start_line {
                sel_start_col
            } else {
                0
            };
            let sel_col_end = if line_idx == sel_end_line {
                sel_end_col
            } else {
                usize::MAX
            };

            if let Some(line) = lines.get_mut(line_idx) {
                apply_line_selection(line, sel_col_start, sel_col_end, theme.selection_bg);
            }
        }
    }

    let line_texts: Vec<String> = lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>()
        })
        .collect();

    lines.push(spacer());
    BuildResult {
        text: Text::from(lines),
        click_ranges,
        line_texts,
    }
}

/// Classify a message into a role category for spacing decisions.
fn msg_role(msg: &DisplayMessage) -> Option<Role> {
    match msg {
        DisplayMessage::Chat { role, .. } => Some(*role),
        DisplayMessage::Tool { .. } => Some(Role::Tool),
        DisplayMessage::Diff { .. } => None  /* diff */,
    }
}

fn toggle_style(theme: &super::theme::Theme, hovered: bool) -> Style {
    let base = Style::default()
        .fg(theme.accent)
        .add_modifier(Modifier::ITALIC);
    if hovered {
        base.add_modifier(Modifier::UNDERLINED)
    } else {
        base
    }
}

fn render_assistant_message(
    lines: &mut Vec<Line>,
    state: &AppState,
    content: &str,
    thinking: Option<&str>,
    max_w: usize,
) {
    let theme = state.theme();

    if let Some(think) = thinking
        && state.display.thinking_visible
        && !think.is_empty()
    {
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
        let think_w = max_w.saturating_sub(UnicodeWidthStr::width(THINKING_RAIL));
        let think_style = Style::default().fg(theme.thinking);
        for think_line in think.lines() {
            for chunk in wrap_str(think_line, think_w) {
                lines.push(Line::from(Span::styled(
                    format!("{}{}", THINKING_RAIL, chunk),
                    think_style,
                )));
            }
        }
    }

    lines.extend(glyph_lines(
        content,
        ASSISTANT_GLYPH,
        theme.assistant,
        theme.input,
        theme.rail,
        max_w,
    ));
}

fn render_system_message(
    lines: &mut Vec<Line>,
    content: &str,
    collapsed: bool,
    theme: &super::theme::Theme,
    max_w: usize,
) {
    let style = Style::default().fg(theme.system);
    let prefix_w = UnicodeWidthStr::width(TOOL_GLYPH);
    let content_w = max_w.saturating_sub(prefix_w);

    let all_lines: Vec<&str> = content.lines().collect();

    if collapsed {
        let (prefix, line) = if all_lines.len() == 1 {
            (TOOL_GLYPH, all_lines[0])
        } else {
            (CARD_TOP, all_lines[0])
        };
        for chunk in wrap_str(line, content_w) {
            lines.push(Line::from(vec![
                Span::styled(prefix.to_string(), style),
                Span::styled(chunk, style),
            ]));
        }
        if all_lines.len() > 1 {
            lines.push(Line::from(vec![
                Span::styled(CARD_BOT.to_string(), style),
                Span::styled(
                    format!("+{} lines ", all_lines.len().saturating_sub(1)),
                    Style::default()
                        .fg(theme.system)
                        .add_modifier(Modifier::ITALIC),
                ),
            ]));
        }
    } else {
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
            for chunk in wrap_str(line, content_w) {
                lines.push(Line::from(vec![
                    Span::styled(prefix.to_string(), style),
                    Span::styled(chunk, style),
                ]));
            }
        }
    }
}
