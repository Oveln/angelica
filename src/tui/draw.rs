use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
        StatefulWidget,
    },
};
use unicode_width::UnicodeWidthStr;

use super::render::build_all_lines;
use super::state::AppState;
use super::theme::{APP_NAME, APP_TAGLINE, PROMPT, Theme, logo_lines};
use super::types::*;

pub fn draw(f: &mut Frame, state: &mut AppState) {
    let theme = *state.theme();
    let status_height: u16 = 1;

    let input_area_height = match &state.mode {
        AppMode::Approval { .. } => {
            let feedback_bonus = if state.approval_selected == ApprovalChoice::EditFeedback {
                3
            } else {
                0
            };
            2 + feedback_bonus + 3
        }
        _ => 3,
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),
            Constraint::Length(input_area_height),
            Constraint::Length(status_height),
        ])
        .split(f.area());

    let show_welcome = state.messages.is_empty()
        && state.thinking_buffer.is_empty()
        && state.text_buffer.is_empty()
        && !state.is_streaming;

    if show_welcome {
        draw_welcome(f, &theme, chunks[0]);
    } else {
        draw_messages(
            f,
            state,
            chunks[0],
            chunks[0].width.saturating_sub(1) as usize,
        );
    }

    match &state.mode {
        AppMode::Approval { tool_label, .. } => {
            let has_feedback = state.approval_selected == ApprovalChoice::EditFeedback;
            let mut constraints = vec![Constraint::Length(2)];
            if has_feedback {
                constraints.push(Constraint::Length(3));
            }
            constraints.push(Constraint::Length(3));

            let approval_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints(constraints)
                .split(chunks[1]);

            draw_approval_header(f, approval_chunks[0], tool_label, &theme);
            let input_idx = if has_feedback {
                draw_feedback_input(f, state, approval_chunks[1], &theme);
                2
            } else {
                1
            };
            draw_approval_choices(f, state, approval_chunks[input_idx], &theme);
        }
        _ => {
            draw_input(f, state, chunks[1], &theme);
        }
    }

    draw_status_bar(f, state, chunks[2], &theme);

    if state.mode == AppMode::SlashMenu {
        draw_slash_menu(f, state, f.area(), &theme);
    }

    if !state.queued_messages.is_empty() {
        let queue_area = Rect {
            x: f.area().x,
            y: f.area().bottom().saturating_sub(4),
            width: f.area().width,
            height: 1,
        };
        let label = if state.queued_messages.len() == 1 {
            "1 queued message".to_string()
        } else {
            format!("{} queued messages", state.queued_messages.len())
        };
        let queue_para = Paragraph::new(Line::from(Span::styled(
            format!("  \u{25B8} {} (Enter to edit, Esc to cancel)", label),
            Style::default().fg(theme.warning),
        )));
        f.render_widget(Clear, queue_area);
        f.render_widget(queue_para, queue_area);
    }
}

fn draw_welcome(f: &mut Frame, theme: &Theme, area: Rect) {
    let logo = logo_lines();
    let logo_height = logo.len() as u16;
    let tagline = APP_TAGLINE;
    let tips = [
        "Type a message to start a conversation",
        "/ for commands  \u{2502}  ? for help",
    ];

    let total_content = logo_height + 1 + 1 + 1 + tips.len() as u16;
    let top_pad = area.height.saturating_sub(total_content) / 2;

    let mut lines: Vec<Line> = Vec::new();

    for _ in 0..top_pad {
        lines.push(Line::from(""));
    }

    let logo_width: u16 = logo
        .iter()
        .map(|l| UnicodeWidthStr::width(*l) as u16)
        .max()
        .unwrap_or(30);
    let center_offset = area.width.saturating_sub(logo_width) / 2;
    let pad_str = " ".repeat(center_offset as usize);

    for line in &logo {
        let trimmed = line.trim_end();
        lines.push(Line::from(Span::styled(
            format!("{}{}", pad_str, trimmed),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )));
    }

    lines.push(Line::from(""));
    let tagline_width = UnicodeWidthStr::width(tagline) as u16;
    lines.push(Line::from(Span::styled(
        format!(
            "{}{}",
            " ".repeat(area.width.saturating_sub(tagline_width) as usize / 2),
            tagline
        ),
        Style::default().fg(theme.muted),
    )));
    lines.push(Line::from(""));

    for tip in &tips {
        let tip_width = UnicodeWidthStr::width(*tip) as u16;
        lines.push(Line::from(Span::styled(
            format!(
                "{}{}",
                " ".repeat(area.width.saturating_sub(tip_width) as usize / 2),
                tip
            ),
            Style::default().fg(theme.status_muted),
        )));
    }

    f.render_widget(Paragraph::new(lines), area);
}

fn draw_status_bar(f: &mut Frame, state: &AppState, area: Rect, theme: &Theme) {
    let mode_indicator = match &state.mode {
        AppMode::Chat => "\u{25CB} idle",
        AppMode::Streaming => "\u{25CF} streaming",
        AppMode::Approval { .. } => "\u{25D0} approval",
        AppMode::SlashMenu => "\u{25CB} idle",
    };
    let mode_style = match &state.mode {
        AppMode::Streaming => Style::default().fg(theme.success),
        AppMode::Approval { .. } => Style::default().fg(theme.warning),
        _ => Style::default().fg(theme.status_muted),
    };

    let msg_count = state.messages.len();

    let left_parts: Vec<Span> = vec![
        Span::styled(
            format!(" {} ", APP_NAME),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("\u{2502}", Style::default().fg(theme.status_muted)),
        Span::styled(
            format!(" {} ", state.model_name),
            Style::default().fg(theme.status_fg),
        ),
        Span::styled("\u{2502}", Style::default().fg(theme.status_muted)),
        Span::styled(format!(" {} ", mode_indicator), mode_style),
        Span::styled("\u{2502}", Style::default().fg(theme.status_muted)),
        Span::styled(
            format!(" {} msgs ", msg_count),
            Style::default().fg(theme.status_muted),
        ),
    ];

    let thinking_label = if state.thinking_visible { "on" } else { "off" };
    let right_parts: Vec<Span> = vec![
        Span::styled(
            format!("verbose: {} ", state.verbosity.label()),
            Style::default().fg(theme.status_muted),
        ),
        Span::styled("\u{2502}", Style::default().fg(theme.status_muted)),
        Span::styled(
            format!(" thinking: {} ", thinking_label),
            Style::default().fg(theme.status_muted),
        ),
    ];

    let left_width: usize = left_parts.iter().map(|s| s.content.len()).sum();
    let right_width: usize = right_parts.iter().map(|s| s.content.len()).sum();
    let gap = area.width as usize;
    let fill = gap.saturating_sub(left_width + right_width);

    let mut spans: Vec<Span> = left_parts;
    spans.push(Span::styled(
        " ".repeat(fill),
        Style::default().fg(theme.status_muted),
    ));
    spans.extend(right_parts);

    let status_line = Paragraph::new(Line::from(spans)).style(Style::default().bg(theme.status_bg));
    f.render_widget(status_line, area);
}

fn draw_messages(f: &mut Frame, state: &mut AppState, area: Rect, terminal_width: usize) {
    let result = build_all_lines(state, terminal_width);
    let text = result.text;
    let content_height = text.height();
    let visible_height = area.height as usize;

    state.apply_pending_scroll(content_height, visible_height);

    let max_start = content_height.saturating_sub(visible_height);

    let top = state.resolve_top(max_start);
    let at_tail = top >= max_start || state.is_at_tail();

    let end = if at_tail {
        max_start + visible_height
    } else {
        top + visible_height
    };
    let end = end.min(content_height);
    let visible_lines: Vec<Line> = if content_height == 0 {
        vec![Line::from("")]
    } else {
        text.lines[top..end].to_vec()
    };

    let padded = if at_tail && visible_lines.len() < visible_height {
        let pad = visible_height - visible_lines.len();
        let mut v: Vec<Line> = (0..pad).map(|_| Line::from("")).collect();
        v.extend(visible_lines);
        v
    } else {
        visible_lines
    };

    state.clickable_ranges = result.click_ranges;
    state.cached_line_texts = result.line_texts;
    state.content_top = top;
    state.content_height = content_height;
    state.messages_area = area;

    let paragraph = Paragraph::new(padded);
    f.render_widget(paragraph, area);

    let theme = state.theme();
    if content_height > visible_height && area.width > 1 {
        let scrollable = content_height.saturating_sub(visible_height);
        let pos = if at_tail {
            scrollable
        } else {
            top.min(scrollable)
        };
        let mut sb_state = ScrollbarState::new(scrollable)
            .position(pos)
            .viewport_content_length(visible_height);
        Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_symbol(Some("\u{2502}"))
            .track_style(Style::default().fg(theme.rail))
            .thumb_symbol("\u{2503}")
            .thumb_style(Style::default().fg(theme.muted))
            .render(area, f.buffer_mut(), &mut sb_state);
    }
}

fn draw_input(f: &mut Frame, state: &AppState, area: Rect, theme: &Theme) {
    let is_approval = matches!(state.mode, AppMode::Approval { .. });
    let border_color = if is_approval {
        theme.warning
    } else if state.is_streaming {
        theme.border_active
    } else {
        theme.border
    };

    let (prompt_str, prompt_color) = if state.is_streaming && !state.queued_messages.is_empty() {
        (
            format!("\u{276F} {} queued \u{2190} ", state.queued_messages.len()),
            theme.muted,
        )
    } else {
        (PROMPT.to_string(), theme.prompt)
    };
    let prompt_width = UnicodeWidthStr::width(prompt_str.as_str()) as u16;

    let input_fg = if state.is_streaming && !state.input.is_empty() {
        theme.muted
    } else {
        theme.input
    };
    let input_spans = vec![
        Span::styled(
            prompt_str,
            Style::default()
                .fg(prompt_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(state.input.as_str(), Style::default().fg(input_fg)),
    ];

    let content = Line::from(input_spans);
    let input = Paragraph::new(content).block(
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(border_color)),
    );
    f.render_widget(input, area);

    let display_col = state.input.display_cursor_col();
    f.set_cursor_position((area.x + prompt_width + display_col, area.y + 1));
}

fn draw_approval_header(f: &mut Frame, area: Rect, tool_label: &str, theme: &Theme) {
    let header = Line::from(vec![
        Span::styled(" \u{25B3} ", Style::default().fg(theme.warning)),
        Span::styled(
            "Permission required",
            Style::default()
                .fg(theme.input)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    let max_w = area.width as usize;
    let label_w = max_w.saturating_sub(3);
    let truncated: String = tool_label.chars().take(label_w).collect();
    let detail = Line::from(vec![
        Span::styled("   ", Style::default()),
        Span::styled(truncated, Style::default().fg(theme.muted)),
    ]);
    let para = Paragraph::new(vec![header, detail]).style(Style::default().bg(theme.status_bg));
    f.render_widget(para, area);
}

fn draw_approval_choices(f: &mut Frame, state: &AppState, area: Rect, theme: &Theme) {
    let selected = state.approval_selected;
    let max_w = area.width as usize;
    let choices: Vec<Span> = ApprovalChoice::ALL
        .iter()
        .flat_map(|&choice| {
            let label = choice.label();
            let sel = choice == selected;
            let hint = match choice {
                ApprovalChoice::Allow => "y",
                ApprovalChoice::Reject => "n",
                ApprovalChoice::EditFeedback => "e",
            };
            let styled = if sel {
                format!(" \u{25B8} {} [{}] \u{25C2} ", label, hint)
            } else {
                format!("   {} [{}]   ", label, hint)
            };
            vec![Span::styled(styled, choice.style(sel, theme))]
        })
        .collect();

    let editing = state.approval_selected == ApprovalChoice::EditFeedback
        && matches!(state.mode, AppMode::Approval { .. });
    let hint_text = if editing {
        "enter confirm  \u{2502}  esc back"
    } else {
        "\u{2194} select  \u{2502}  y/n confirm"
    };
    let hint_str = format!("  {}", hint_text);
    let hint_w = UnicodeWidthStr::width(hint_str.as_str());
    let hint_display = if hint_w > max_w {
        let truncated: String = hint_str.chars().take(max_w).collect();
        truncated
    } else {
        hint_str
    };
    let hints = Line::from(Span::styled(
        hint_display,
        Style::default().fg(theme.status_muted),
    ));

    let para = Paragraph::new(vec![Line::from(""), Line::from(choices), hints]);
    f.render_widget(para, area);
}

fn draw_feedback_input(f: &mut Frame, state: &AppState, area: Rect, theme: &Theme) {
    let feedback_block = Block::default()
        .borders(Borders::ALL)
        .title(" Feedback ")
        .border_style(Style::default().fg(theme.warning));
    let feedback_para = Paragraph::new(Span::styled(
        state.feedback.as_str(),
        Style::default().fg(theme.input),
    ))
    .block(feedback_block);
    f.render_widget(feedback_para, area);

    let fb_col = state.feedback.display_cursor_col();
    f.set_cursor_position((area.x + 1 + fb_col, area.y + 1));
}

fn draw_slash_menu(f: &mut Frame, state: &AppState, area: Rect, theme: &Theme) {
    if state.slash_matched.is_empty() {
        return;
    }

    let menu_width: u16 = 48;
    let menu_height = (state.slash_matched.len().min(8) as u16) + 2;
    let menu_area = Rect {
        x: area.x + 2,
        y: area.bottom().saturating_sub(3 + menu_height),
        width: menu_width.min(area.width),
        height: menu_height,
    };

    f.render_widget(Clear, menu_area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border));
    f.render_widget(block, menu_area);

    let inner = Rect {
        x: menu_area.x + 1,
        y: menu_area.y + 1,
        width: menu_area.width.saturating_sub(2),
        height: menu_area.height.saturating_sub(2),
    };

    let name_col_width = 20usize;
    let inner_w = inner.width as usize;
    let items: Vec<Line> = state
        .slash_matched
        .iter()
        .enumerate()
        .map(|(vi, &ci)| {
            let cmd = &BUILTIN_COMMANDS[ci];
            let sel = vi == state.slash_selected;
            let name_str = if cmd.aliases.is_empty() {
                cmd.name.to_string()
            } else {
                format!("{} ({})", cmd.name, cmd.aliases.join(", "))
            };
            let name_padded = format!("{:<width$}", name_str, width = name_col_width);
            let max_desc = inner_w.saturating_sub(name_col_width + 4);
            let desc_display: String = cmd.description.chars().take(max_desc).collect();

            if sel {
                Line::from(vec![
                    Span::styled(
                        format!(" \u{25B8} {}", name_padded),
                        Style::default()
                            .fg(ratatui::style::Color::Black)
                            .bg(theme.tool)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        desc_display,
                        Style::default()
                            .fg(ratatui::style::Color::Black)
                            .bg(theme.tool),
                    ),
                    Span::styled(" ".repeat(inner_w), Style::default().bg(theme.tool)),
                ])
            } else {
                Line::from(vec![
                    Span::styled(
                        format!("   {}", name_padded),
                        Style::default().fg(theme.tool),
                    ),
                    Span::styled(desc_display, Style::default().fg(theme.muted)),
                ])
            }
        })
        .collect();

    let para = Paragraph::new(items);
    f.render_widget(para, inner);
}
