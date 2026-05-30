use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
        Sparkline, StatefulWidget,
    },
};
use unicode_width::UnicodeWidthStr;

use super::mode::{self, AppMode, ApprovalChoice};
use super::render::build_all_lines;
use super::state::AppState;
use super::theme::{APP_NAME, APP_TAGLINE, PROMPT, Theme, logo_lines};

const STATUS_PANEL_WIDTH: u16 = 26;

pub fn draw(f: &mut Frame, state: &mut AppState) {
    let theme = *state.theme();
    let status_height: u16 = 1;

    let input_area_height = match &state.mode {
        AppMode::Welcome => 3,
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

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),
            Constraint::Length(input_area_height),
            Constraint::Length(status_height),
        ])
        .split(f.area());

    let show_panel = !matches!(state.mode, AppMode::Welcome) && f.area().width > 64;

    let msgs_area = if show_panel {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(20), Constraint::Length(STATUS_PANEL_WIDTH)])
            .split(outer[0]);
        draw_status_panel(f, &theme, cols[1], state);
        cols[0]
    } else {
        outer[0]
    };

    if matches!(state.mode, AppMode::Welcome) {
        draw_welcome(f, &theme, outer[0]);
    } else {
        draw_messages(
            f,
            state,
            msgs_area,
            msgs_area.width.saturating_sub(1) as usize,
        );
    }

    match &state.mode {
        AppMode::Welcome => {
            draw_input(f, state, outer[1], &theme);
        }
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
                .split(outer[1]);

            mode::approval::draw_header(f, approval_chunks[0], &a.tool_label, &theme);
            let input_idx = if has_feedback {
                mode::approval::draw_feedback_input(f, state, approval_chunks[1], &theme);
                2
            } else {
                1
            };
            mode::approval::draw_choices(f, state, approval_chunks[input_idx], &theme);
        }
        _ => {
            draw_input(f, state, outer[1], &theme);
        }
    }

    draw_status_bar(f, state, outer[2], &theme);

    if matches!(state.mode, AppMode::SlashMenu(_)) {
        mode::slash::draw(f, state, f.area(), &theme);
    }

    if matches!(state.mode, AppMode::UsageStats) {
        draw_usage_stats(f, state, f.area(), &theme);
    }
    if matches!(state.mode, AppMode::Settings(_)) {
        crate::mode::settings::draw(f, state, f.area(), &theme);
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
    let tips = ["Press any key to wake up"];

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

fn draw_status_panel(f: &mut Frame, theme: &Theme, area: Rect, state: &AppState) {
    let bar_width = (area.width as usize).saturating_sub(4);
    let filled = (state.fatigue.fatigue * bar_width as f64).round() as usize;
    let empty = bar_width.saturating_sub(filled);
    let bar = format!("{}{}", "\u{2588}".repeat(filled), "\u{2591}".repeat(empty));

    let fatigue_pct = format!("{:.0}%", state.fatigue.fatigue * 100.0);

    let mut lines = vec![
        Line::from(Span::styled(
            " \u{25CF} \u{72B6}\u{6001}",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            format!(" {}", state.fatigue.desc),
            Style::default().fg(theme.status_fg),
        )),
        Line::from(""),
        Line::from(Span::styled(
            format!(" {}", bar),
            Style::default().fg(theme.muted),
        )),
        Line::from(Span::styled(
            format!(
                " {}\u{258E}{}",
                " ".repeat(bar_width.saturating_sub(fatigue_pct.len()) / 2),
                fatigue_pct
            ),
            Style::default().fg(theme.status_muted),
        )),
        Line::from(""),
        Line::from(Span::styled(
            format!(" \u{21BB} {} turns", state.fatigue.turns),
            Style::default().fg(theme.status_muted),
        )),
        Line::from(Span::styled(
            format!(" \u{2699} {} calls", state.fatigue.tool_calls),
            Style::default().fg(theme.status_muted),
        )),
        Line::from(""),
        Line::from(Span::styled(
            " \u{25C6} \u{7528}\u{91CF}",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    let u = &state.usage;
    let prompt_k = format_tokens(u.prompt_tokens);
    let completion_k = format_tokens(u.completion_tokens);
    let total_k = format_tokens(u.total_tokens);

    lines.push(Line::from(Span::styled(
        format!(" in:  {}k", prompt_k),
        Style::default().fg(theme.status_muted),
    )));
    lines.push(Line::from(Span::styled(
        format!(" out: {}k", completion_k),
        Style::default().fg(theme.status_muted),
    )));
    lines.push(Line::from(Span::styled(
        format!(" sum: {}k", total_k),
        Style::default().fg(theme.status_muted),
    )));
    lines.push(Line::from(""));

    if u.cache_hit_tokens > 0 || u.cache_miss_tokens > 0 {
        let hit_k = format_tokens(u.cache_hit_tokens);
        let miss_k = format_tokens(u.cache_miss_tokens);
        let rate_pct = format!("{:.0}%", u.cache_hit_rate() * 100.0);
        let cache_bar_filled = if u.cache_total() > 0 {
            (u.cache_hit_rate() * bar_width as f64).round() as usize
        } else {
            0
        };
        let cache_bar_empty = bar_width.saturating_sub(cache_bar_filled);
        let cache_bar = format!(
            "{}{}",
            "\u{2588}".repeat(cache_bar_filled),
            "\u{2591}".repeat(cache_bar_empty)
        );

        lines.push(Line::from(Span::styled(
            format!(" hit:  {}k", hit_k),
            Style::default().fg(theme.status_muted),
        )));
        lines.push(Line::from(Span::styled(
            format!(" miss: {}k", miss_k),
            Style::default().fg(theme.status_muted),
        )));
        lines.push(Line::from(Span::styled(
            format!(" {}", cache_bar),
            Style::default().fg(theme.muted),
        )));
        lines.push(Line::from(Span::styled(
            format!(
                " {}\u{258E}{}",
                " ".repeat(bar_width.saturating_sub(rate_pct.len()) / 2),
                rate_pct
            ),
            Style::default().fg(theme.status_muted),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            " cache: --",
            Style::default().fg(theme.status_muted),
        )));
    }

    let panel = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::LEFT)
            .border_style(Style::default().fg(theme.border)),
    );
    f.render_widget(panel, area);
}

fn draw_status_bar(f: &mut Frame, state: &AppState, area: Rect, theme: &Theme) {
    let mode_indicator = match &state.mode {
        AppMode::Welcome => "\u{25CB} sleeping",
        AppMode::Chat => "\u{25CB} idle",
        AppMode::Streaming => "\u{25CF} streaming",
        AppMode::Approval(_) => "\u{25D0} approval",
        AppMode::SlashMenu(_) => "\u{25CB} idle",
        AppMode::UsageStats => "\u{25C6} usage",
        AppMode::Settings(_) => "\u{2699} settings",
    };
    let mode_style = match &state.mode {
        AppMode::Streaming => Style::default().fg(theme.success),
        AppMode::Approval(_) => Style::default().fg(theme.warning),
        AppMode::UsageStats => Style::default().fg(theme.accent),
        AppMode::Settings(_) => Style::default().fg(theme.accent),
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

    let left_width: usize = left_parts.iter().map(|s| UnicodeWidthStr::width(s.content.as_ref())).sum();
    let right_width: usize = right_parts.iter().map(|s| UnicodeWidthStr::width(s.content.as_ref())).sum();
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

fn format_tokens(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else {
        format!("{:.1}", tokens as f64 / 1_000.0)
    }
}

fn draw_usage_stats(f: &mut Frame, state: &AppState, area: Rect, theme: &Theme) {
    let sessions = state.cached_usage_sessions.as_deref().unwrap_or_default();

    let popup_area = centered_rect(area, 70, 80);
    // Clear slightly wider than popup to avoid CJK full-width characters
    // straddling the border from the background
    let clear_area = Rect {
        x: popup_area.x.saturating_sub(1),
        y: popup_area.y,
        width: popup_area.width.saturating_add(2).min(area.width),
        height: popup_area.height,
    };
    f.render_widget(Clear, clear_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent))
        .title(Span::styled(
            " Usage Statistics ",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ))
        .title_style(Style::default().fg(theme.accent));
    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    if sessions.is_empty() {
        let empty = Paragraph::new(Line::from(Span::styled(
            "No usage data yet. Start a conversation to see stats.",
            Style::default().fg(theme.muted),
        )));
        f.render_widget(empty, inner);
        return;
    }

    let chart_data_count = 30usize;

    // Split into header + sparkline charts + table
    let chart_height = 3u16; // 3 sparklines
    let header_height = 2u16;
    let remaining = inner
        .height
        .saturating_sub(header_height + chart_height * 3);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(header_height),
            Constraint::Length(chart_height),
            Constraint::Length(chart_height),
            Constraint::Length(chart_height),
            Constraint::Min(0),
        ])
        .split(inner);

    // Header
    let header_lines = vec![
        Line::from(Span::styled(
            format!(" {} sessions loaded  |  Esc to close", sessions.len()),
            Style::default().fg(theme.status_fg),
        )),
        Line::from(""),
    ];
    f.render_widget(Paragraph::new(header_lines), chunks[0]);

    // Prepare chart data: last N sessions
    let recent: Vec<_> = sessions
        .iter()
        .rev()
        .take(chart_data_count)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    // Sparkline 1: Total tokens per session
    let total_data: Vec<u64> = recent.iter().map(|s| s.total_tokens).collect();
    let max_total = total_data.iter().copied().max().unwrap_or(1);
    let spark1 = Sparkline::default()
        .data(&total_data)
        .style(Style::default().fg(theme.accent))
        .max(max_total);
    let spark1_block = Block::default()
        .title(Span::styled(
            format!(" Total tokens (max {}k)", max_total / 1000),
            Style::default().fg(theme.status_muted),
        ))
        .borders(Borders::NONE);
    f.render_widget(spark1, spark1_block.inner(chunks[1]));
    f.render_widget(spark1_block, chunks[1]);

    // Sparkline 2: Cache hit rate per session (as percentage * 100)
    let hit_data: Vec<u64> = recent
        .iter()
        .map(|s| (s.cache_hit_rate() * 100.0).round() as u64)
        .collect();
    let spark2 = Sparkline::default()
        .data(&hit_data)
        .style(Style::default().fg(theme.success))
        .max(100);
    let spark2_block = Block::default()
        .title(Span::styled(
            " Cache hit rate %",
            Style::default().fg(theme.status_muted),
        ))
        .borders(Borders::NONE);
    f.render_widget(spark2, spark2_block.inner(chunks[2]));
    f.render_widget(spark2_block, chunks[2]);

    // Sparkline 3: Completion tokens per session
    let comp_data: Vec<u64> = recent.iter().map(|s| s.completion_tokens).collect();
    let max_comp = comp_data.iter().copied().max().unwrap_or(1);
    let spark3 = Sparkline::default()
        .data(&comp_data)
        .style(Style::default().fg(theme.warning))
        .max(max_comp);
    let spark3_block = Block::default()
        .title(Span::styled(
            format!(" Output tokens (max {}k)", max_comp / 1000),
            Style::default().fg(theme.status_muted),
        ))
        .borders(Borders::NONE);
    f.render_widget(spark3, spark3_block.inner(chunks[3]));
    f.render_widget(spark3_block, chunks[3]);

    // Session table
    let max_rows = remaining as usize;
    let display_sessions: Vec<_> = sessions.iter().rev().take(max_rows).collect();

    let mut table_lines: Vec<Line> = Vec::new();
    table_lines.push(Line::from(vec![
        Span::styled(
            format!(" {:<5}", "#"),
            Style::default()
                .fg(theme.status_muted)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {:<6}", "Type"),
            Style::default()
                .fg(theme.status_muted)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {:>10}", "In"),
            Style::default()
                .fg(theme.status_muted)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {:>10}", "Out"),
            Style::default()
                .fg(theme.status_muted)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {:>10}", "Total"),
            Style::default()
                .fg(theme.status_muted)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {:>6}", "Iters"),
            Style::default()
                .fg(theme.status_muted)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {:>6}", "Cache%"),
            Style::default()
                .fg(theme.status_muted)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  {}", "Time"),
            Style::default()
                .fg(theme.status_muted)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    for (i, s) in display_sessions.iter().enumerate() {
        let short_time = s
            .start_time
            .get(..16)
            .unwrap_or(&s.start_time)
            .replace('T', " ");
        let scope_label = if s.scope == angelica::usage::UsageScope::Awake {
            "\u{25CF} awake"
        } else {
            "\u{25CB} sleep"
        };
        let scope_style = if s.scope == angelica::usage::UsageScope::Awake {
            Style::default().fg(theme.success)
        } else {
            Style::default().fg(theme.status_muted)
        };
        let row_style = if i % 2 == 0 {
            Style::default()
        } else {
            Style::default().bg(theme.status_bg)
        };

        table_lines.push(Line::from(vec![
            Span::styled(format!(" {:<5}", i + 1), row_style),
            Span::styled(format!(" {:<6}", scope_label), scope_style),
            Span::styled(format!(" {:>9}k", s.prompt_tokens / 1000), row_style),
            Span::styled(format!(" {:>9}k", s.completion_tokens / 1000), row_style),
            Span::styled(format!(" {:>9}k", s.total_tokens / 1000), row_style),
            Span::styled(format!(" {:>6}", s.iterations), row_style),
            Span::styled(format!(" {:>5.0}%", s.cache_hit_rate() * 100.0), row_style),
            Span::styled(format!("  {}", short_time), row_style),
        ]));
    }

    f.render_widget(Paragraph::new(table_lines), chunks[4]);
}

fn centered_rect(area: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let popup_width = area.width * percent_x / 100;
    let popup_height = area.height * percent_y / 100;
    Rect {
        x: area.x + (area.width.saturating_sub(popup_width)) / 2,
        y: area.y + (area.height.saturating_sub(popup_height)) / 2,
        width: popup_width,
        height: popup_height,
    }
}
