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

use super::mode::{self, AppMode, ApprovalChoice};
use super::render::build_all_lines;
use super::state::AppState;
use super::theme::{APP_NAME, APP_TAGLINE, PROMPT, Theme, logo_lines};

pub fn draw(f: &mut Frame, state: &mut AppState) {
    let theme = *state.theme();
    let status_height: u16 = 1;

    let input_area_height = match &state.mode {
        AppMode::Approval(a) => {
            let feedback_bonus = if a.selected == ApprovalChoice::EditFeedback {
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
        AppMode::Approval(a) => {
            let has_feedback = a.selected == ApprovalChoice::EditFeedback;
            let mut constraints = vec![Constraint::Length(2)];
            if has_feedback {
                constraints.push(Constraint::Length(3));
            }
            constraints.push(Constraint::Length(3));

            let approval_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints(constraints)
                .split(chunks[1]);

            mode::approval::draw_header(f, approval_chunks[0], &a.tool_label, &theme);
            let input_idx = if has_feedback {
                mode::approval::draw_feedback_input(f, state, approval_chunks[1], &theme);
                2
            } else {
                1
            };
            mode::approval::draw_choices(f, state, approval_chunks[input_idx], &theme);
        }
        AppMode::SessionPicker(sp) => {
            mode::session::draw(f, state, f.area(), &theme);

            let filter = &sp.filter;
            let prompt = "filter: ";
            let prompt_w = UnicodeWidthStr::width(prompt) as u16;
            let filter_w = UnicodeWidthStr::width(filter.as_str()) as u16;
            let border_color = theme.accent;

            let input_spans = vec![
                Span::styled(
                    prompt,
                    Style::default()
                        .fg(theme.muted)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(filter.clone(), Style::default().fg(theme.input)),
            ];
            let content = Line::from(input_spans);
            let input = Paragraph::new(content).block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_style(Style::default().fg(border_color)),
            );
            f.render_widget(input, chunks[1]);

            let cursor_x = chunks[1].x + prompt_w + filter_w;
            let cursor_y = chunks[1].y + 1;
            f.set_cursor_position((cursor_x, cursor_y));
        }
        _ => {
            draw_input(f, state, chunks[1], &theme);
        }
    }

    draw_status_bar(f, state, chunks[2], &theme);

    if matches!(state.mode, AppMode::SlashMenu(_)) {
        mode::slash::draw(f, state, f.area(), &theme);
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
        AppMode::Approval(_) => "\u{25D0} approval",
        AppMode::SlashMenu(_) => "\u{25CB} idle",
        AppMode::SessionPicker(_) => "\u{25CB} resume",
    };
    let mode_style = match &state.mode {
        AppMode::Streaming => Style::default().fg(theme.success),
        AppMode::Approval(_) => Style::default().fg(theme.warning),
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

    let thinking_label = if state.display.thinking_visible {
        "on"
    } else {
        "off"
    };
    let right_parts: Vec<Span> = vec![
        Span::styled(
            format!("verbose: {} ", state.display.verbosity.label()),
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

    state.scroll.apply_pending(content_height, visible_height);

    let max_start = content_height.saturating_sub(visible_height);

    let top = state.scroll.resolve_top(max_start);
    let at_tail = top >= max_start || state.scroll.is_at_tail();

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

    state.viewport.clickable_ranges = result.click_ranges;
    state.viewport.cached_line_texts = result.line_texts;
    state.viewport.content_top = top;
    state.viewport.content_height = content_height;
    state.viewport.messages_area = area;

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
    let is_approval = matches!(state.mode, AppMode::Approval(_));
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
