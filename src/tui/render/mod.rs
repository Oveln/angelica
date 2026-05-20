mod cards;
mod text;

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use unicode_width::UnicodeWidthStr;

use super::state::AppState;
use super::theme::{
    ASSISTANT_GLYPH, CARD_BOT, CARD_MID, CARD_TOP, RAIL, THINKING_RAIL, TOOL_GLYPH, USER_GLYPH,
};
use super::types::{ClickRange, DisplayMessage};

use cards::render_inline_tool;
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

    for (msg_idx, msg) in state.messages.iter().enumerate() {
        if msg.is_hidden() {
            continue;
        }
        if !lines.is_empty() {
            lines.push(Line::from(""));
        }
        let line_count_before = lines.len();

        match msg {
            DisplayMessage::Chat {
                role,
                content,
                thinking,
                collapsed,
                ..
            } => match role.as_str() {
                "user" => render_glyph_lines(
                    &mut lines,
                    content,
                    USER_GLYPH,
                    theme.user,
                    theme.user,
                    theme.rail,
                    terminal_width,
                ),
                "assistant" => {
                    render_assistant_message(
                        &mut lines,
                        state,
                        content,
                        thinking.as_deref(),
                        terminal_width,
                    );
                }
                "system" => {
                    render_system_message(&mut lines, content, *collapsed, theme, terminal_width);
                }
                _ => {
                    lines.push(Line::from(content.clone()));
                }
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
        let think_w = terminal_width.saturating_sub(UnicodeWidthStr::width(THINKING_RAIL));
        let think_style = Style::default().fg(theme.thinking);
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
        let body_style = Style::default().fg(theme.input);
        let content_w = terminal_width.saturating_sub(UnicodeWidthStr::width(RAIL));
        for content_line in state.text_buffer.lines() {
            for chunk in wrap_str(content_line, content_w) {
                lines.push(Line::from(vec![
                    Span::styled(RAIL.to_string(), rail_style),
                    Span::styled(chunk, body_style),
                ]));
            }
        }
    }

    if let Some((sl, sc, el, ec)) = state.mouse.selection {
        let (sel_start_line, sel_start_col, sel_end_line, sel_end_col) =
            if sl < el || (sl == el && sc <= ec) {
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

    lines.push(Line::from(""));
    BuildResult {
        text: Text::from(lines),
        click_ranges,
        line_texts,
    }
}

fn toggle_style(theme: &super::theme::Theme, hovered: bool) -> Style {
    let base = Style::default()
        .fg(theme.accent)
        .bg(theme.panel_bg)
        .add_modifier(Modifier::ITALIC);
    if hovered {
        base.add_modifier(Modifier::UNDERLINED)
    } else {
        base
    }
}

fn render_glyph_lines(
    lines: &mut Vec<Line>,
    content: &str,
    glyph: &str,
    glyph_fg: Color,
    body_fg: Color,
    rail_fg: Color,
    max_w: usize,
) {
    let glyph_style = Style::default().fg(glyph_fg).add_modifier(Modifier::BOLD);
    let rail_style = Style::default().fg(rail_fg);
    let body_style = Style::default().fg(body_fg);
    let glyph_prefix_w = UnicodeWidthStr::width(glyph) + 1;
    let rail_prefix_w = UnicodeWidthStr::width(RAIL);

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
        let wrapped = wrap_str(line, content_w);
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
            lines.push(Line::from(spans));
        }
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

    if let Some(think) = thinking {
        if state.display.thinking_visible && !think.is_empty() {
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
    }

    render_glyph_lines(
        lines,
        content,
        ASSISTANT_GLYPH,
        theme.assistant,
        theme.input,
        theme.rail,
        max_w,
    );
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
            let expand_bg = theme.status_bg;
            lines.push(Line::from(vec![
                Span::styled(CARD_BOT.to_string(), style),
                Span::styled(
                    format!("+{} lines ", all_lines.len().saturating_sub(1)),
                    Style::default()
                        .fg(theme.system)
                        .bg(expand_bg)
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
