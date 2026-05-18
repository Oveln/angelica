use std::collections::VecDeque;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
        StatefulWidget,
    },
};
use unicode_width::UnicodeWidthStr;

use crate::agent::events::AppEvent;
use super::constants::{COLOR_INPUT, COLOR_PROMPT, COLOR_TOOL, PROMPT};
use super::render::build_all_lines;

const TAIL_SENTINEL: usize = usize::MAX;

const QUIET_TOOLS: &[&str] = &[
    "read_file",
    "list_dir",
    "query_sessions",
    "update_agent_memory",
    "update_user_profile",
    "update_soul",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalChoice {
    Allow,
    Reject,
    EditFeedback,
}

impl ApprovalChoice {
    pub const ALL: [ApprovalChoice; 3] = [
        ApprovalChoice::Allow,
        ApprovalChoice::Reject,
        ApprovalChoice::EditFeedback,
    ];

    fn label(self) -> &'static str {
        match self {
            ApprovalChoice::Allow => "Allow",
            ApprovalChoice::Reject => "Reject",
            ApprovalChoice::EditFeedback => "Edit feedback",
        }
    }

    fn style(self, selected: bool) -> Style {
        if selected {
            match self {
                ApprovalChoice::Allow => {
                    Style::default().fg(Color::Black).bg(Color::Green).add_modifier(Modifier::BOLD)
                }
                ApprovalChoice::Reject => {
                    Style::default().fg(Color::Black).bg(Color::Red).add_modifier(Modifier::BOLD)
                }
                ApprovalChoice::EditFeedback => {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                }
            }
        } else {
            Style::default().fg(Color::DarkGray)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verbosity {
    Normal,
    Verbose,
    Trace,
}

impl Verbosity {
    pub fn label(self) -> &'static str {
        match self {
            Verbosity::Normal => "normal",
            Verbosity::Verbose => "verbose",
            Verbosity::Trace => "trace",
        }
    }

    pub fn cycle(self) -> Self {
        match self {
            Verbosity::Normal => Verbosity::Verbose,
            Verbosity::Verbose => Verbosity::Trace,
            Verbosity::Trace => Verbosity::Normal,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SlashCommand {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub description: &'static str,
}

pub static BUILTIN_COMMANDS: &[SlashCommand] = &[
    SlashCommand { name: "help", aliases: &["?"], description: "Show available commands" },
    SlashCommand { name: "clear", aliases: &[], description: "Clear conversation history" },
    SlashCommand { name: "quit", aliases: &["q"], description: "Quit the application" },
    SlashCommand { name: "verbose", aliases: &["v"], description: "Cycle display verbosity" },
    SlashCommand { name: "thinking", aliases: &["think"], description: "Toggle thinking display" },
    SlashCommand { name: "model", aliases: &[], description: "Show current model" },
    SlashCommand { name: "history", aliases: &["h"], description: "Show recent history" },
];

#[derive(Debug, Clone)]
pub struct DisplayMessage {
    pub role: String,
    pub content: String,
    pub thinking: Option<String>,
    pub collapsed: bool,
    pub hidden: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AppMode {
    Chat,
    Approval {
        preview: String,
        is_tty_command: bool,
        command: Option<String>,
    },
    Streaming,
    SlashMenu,
}

pub struct AppState {
    pub messages: Vec<DisplayMessage>,
    pub thinking_buffer: String,
    pub text_buffer: String,
    pub mode: AppMode,
    pub input: String,
    pub input_cursor_char: usize,
    pub should_quit: bool,
    pub thinking_visible: bool,
    pub verbosity: Verbosity,
    pub is_streaming: bool,

    pub queued_messages: VecDeque<String>,

    scroll_offset: usize,
    pending_scroll_delta: i32,

    pub approval_selected: ApprovalChoice,
    pub approval_feedback: String,
    pub approval_feedback_cursor: usize,

    pub slash_filter: String,
    pub slash_selected: usize,
    pub slash_matched: Vec<usize>,
}

impl AppState {
    pub fn add_message(&mut self, role: &str, content: &str, thinking: Option<String>) {
        self.messages.push(DisplayMessage {
            role: role.to_string(),
            content: content.to_string(),
            thinking,
            collapsed: false,
            hidden: false,
        });
    }

    pub fn filtered_messages(&self) -> Vec<&DisplayMessage> {
        self.messages
            .iter()
            .filter(|m| !m.hidden)
            .collect()
    }

    fn should_show_tool_result(&self, tool_name: &str) -> bool {
        match self.verbosity {
            Verbosity::Trace => true,
            Verbosity::Verbose => !QUIET_TOOLS.contains(&tool_name),
            Verbosity::Normal => false,
        }
    }

    fn should_show_tool_call(&self, _tool_name: &str) -> bool {
        true
    }

    // ── Input helpers ──

    pub fn input_insert(&mut self, c: char) {
        let byte_pos = Self::char_to_byte(self.input_cursor_char, &self.input);
        self.input.insert(byte_pos, c);
        self.input_cursor_char += 1;
    }

    pub fn input_backspace(&mut self) {
        if self.input_cursor_char == 0 {
            return;
        }
        self.input_cursor_char -= 1;
        let byte_pos = Self::char_to_byte(self.input_cursor_char, &self.input);
        let char_len = self.input[byte_pos..]
            .chars()
            .next()
            .map(|c| c.len_utf8())
            .unwrap_or(0);
        self.input.drain(byte_pos..byte_pos + char_len);
    }

    pub fn input_delete(&mut self) {
        let total = self.input.chars().count();
        if self.input_cursor_char >= total {
            return;
        }
        let byte_pos = Self::char_to_byte(self.input_cursor_char, &self.input);
        let char_len = self.input[byte_pos..]
            .chars()
            .next()
            .map(|c| c.len_utf8())
            .unwrap_or(0);
        self.input.drain(byte_pos..byte_pos + char_len);
    }

    pub fn input_move_left(&mut self) {
        if self.input_cursor_char > 0 {
            self.input_cursor_char -= 1;
        }
    }
    pub fn input_move_right(&mut self) {
        if self.input_cursor_char < self.input.chars().count() {
            self.input_cursor_char += 1;
        }
    }
    pub fn input_move_home(&mut self) {
        self.input_cursor_char = 0;
    }
    pub fn input_move_end(&mut self) {
        self.input_cursor_char = self.input.chars().count();
    }
    pub fn input_clear(&mut self) {
        self.input.clear();
        self.input_cursor_char = 0;
    }

    // ── Approval feedback helpers ──

    pub fn feedback_insert(&mut self, c: char) {
        let byte_pos = Self::char_to_byte(self.approval_feedback_cursor, &self.approval_feedback);
        self.approval_feedback.insert(byte_pos, c);
        self.approval_feedback_cursor += 1;
    }

    pub fn feedback_backspace(&mut self) {
        if self.approval_feedback_cursor == 0 {
            return;
        }
        self.approval_feedback_cursor -= 1;
        let byte_pos = Self::char_to_byte(self.approval_feedback_cursor, &self.approval_feedback);
        let char_len = self.approval_feedback[byte_pos..]
            .chars()
            .next()
            .map(|c| c.len_utf8())
            .unwrap_or(0);
        self.approval_feedback.drain(byte_pos..byte_pos + char_len);
    }

    pub fn feedback_move_left(&mut self) {
        if self.approval_feedback_cursor > 0 {
            self.approval_feedback_cursor -= 1;
        }
    }
    pub fn feedback_move_right(&mut self) {
        if self.approval_feedback_cursor < self.approval_feedback.chars().count() {
            self.approval_feedback_cursor += 1;
        }
    }

    pub fn feedback_clear(&mut self) {
        self.approval_feedback.clear();
        self.approval_feedback_cursor = 0;
    }

    pub fn toggle_last_collapsed(&mut self) {
        for msg in self.messages.iter_mut().rev() {
            if msg.collapsed {
                msg.collapsed = false;
                return;
            }
        }
    }

    fn char_to_byte(char_idx: usize, s: &str) -> usize {
        s.char_indices()
            .nth(char_idx)
            .map(|(i, _)| i)
            .unwrap_or(s.len())
    }

    // ── Slash menu ──

    pub fn update_slash_matches(&mut self) {
        let filter = if self.input.starts_with('/') {
            &self.input[1..]
        } else {
            ""
        };
        self.slash_filter = filter.to_string();
        self.slash_matched.clear();
        for (i, cmd) in BUILTIN_COMMANDS.iter().enumerate() {
            if cmd.name.starts_with(filter)
                || cmd.aliases.iter().any(|a| a.starts_with(filter))
            {
                self.slash_matched.push(i);
            }
        }
        if self.slash_selected >= self.slash_matched.len() {
            self.slash_selected = 0;
        }
    }

    pub fn slash_selected_cmd(&self) -> Option<&SlashCommand> {
        self.slash_matched
            .get(self.slash_selected)
            .and_then(|&i| BUILTIN_COMMANDS.get(i))
    }

    // ── Scroll helpers ──

    fn is_at_tail(&self) -> bool {
        self.scroll_offset == TAIL_SENTINEL
    }

    fn resolve_top(&self, max_start: usize) -> usize {
        if self.scroll_offset == TAIL_SENTINEL {
            max_start
        } else {
            self.scroll_offset.min(max_start)
        }
    }

    pub fn scroll_up(&mut self, n: usize) {
        self.pending_scroll_delta -= n as i32;
    }

    pub fn scroll_down(&mut self, n: usize) {
        self.pending_scroll_delta += n as i32;
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = TAIL_SENTINEL;
        self.pending_scroll_delta = 0;
    }

    fn apply_pending_scroll(&mut self, total_lines: usize, visible_lines: usize) {
        let delta = self.pending_scroll_delta;
        if delta == 0 {
            return;
        }
        self.pending_scroll_delta = 0;

        if total_lines <= visible_lines {
            self.scroll_offset = TAIL_SENTINEL;
            return;
        }

        let max_start = total_lines.saturating_sub(visible_lines);
        let current = if self.scroll_offset == TAIL_SENTINEL {
            max_start
        } else {
            self.scroll_offset.min(max_start)
        };

        let new_top = if delta < 0 {
            current.saturating_sub(delta.unsigned_abs() as usize)
        } else {
            current.saturating_add(delta as usize).min(max_start)
        };

        self.scroll_offset = if new_top >= max_start {
            TAIL_SENTINEL
        } else {
            new_top
        };
    }

    pub fn handle_event(&mut self, event: &AppEvent) {
        match event {
            AppEvent::ThinkingDelta { delta } => {
                self.thinking_buffer.push_str(delta);
                self.mode = AppMode::Streaming;
                self.is_streaming = true;
            }
            AppEvent::TextDelta { delta } => {
                self.text_buffer.push_str(delta);
                self.mode = AppMode::Streaming;
                self.is_streaming = true;
            }
            AppEvent::TextDone { full_text } => {
                let thinking = if self.thinking_buffer.is_empty() {
                    None
                } else {
                    Some(std::mem::take(&mut self.thinking_buffer))
                };
                self.add_message("assistant", full_text, thinking);
                self.text_buffer.clear();
            }
            AppEvent::TurnComplete => {
                self.is_streaming = false;
                self.mode = AppMode::Chat;
                if let Some(msg) = self.queued_messages.pop_front() {
                    self.input_clear();
                    self.add_message("user", &msg, None);
                    // Will be sent by caller
                }
            }
            AppEvent::ToolResult { name, result } => {
                if self.should_show_tool_result(name) {
                    let preview: String = result.chars().take(100).collect();
                    self.add_message("tool", &format!("[{}: {}]", name, preview), None);
                }
            }
            AppEvent::ToolCalling { name, arguments } => {
                if !self.text_buffer.is_empty() {
                    let thinking = if self.thinking_buffer.is_empty() {
                        None
                    } else {
                        Some(std::mem::take(&mut self.thinking_buffer))
                    };
                    let text = std::mem::take(&mut self.text_buffer);
                    self.add_message("assistant", &text, thinking);
                }
                if self.should_show_tool_call(name) {
                    let display = match name.as_str() {
                        "run_command" => {
                            let args: serde_json::Value =
                                serde_json::from_str(arguments).unwrap_or_default();
                            let cmd = args["command"].as_str().unwrap_or("?");
                            format!("$ {}", cmd)
                        }
                        "read_file" => {
                            let args: serde_json::Value =
                                serde_json::from_str(arguments).unwrap_or_default();
                            let path = args["path"].as_str().unwrap_or("?");
                            format!("read {}", path)
                        }
                        "write_file" => {
                            let args: serde_json::Value =
                                serde_json::from_str(arguments).unwrap_or_default();
                            let path = args["path"].as_str().unwrap_or("?");
                            format!("write {}", path)
                        }
                        "edit_file" => {
                            let args: serde_json::Value =
                                serde_json::from_str(arguments).unwrap_or_default();
                            let path = args["path"].as_str().unwrap_or("?");
                            if let Some(count) = args["count"].as_u64() {
                                format!("edit {} ({} changes)", path, count)
                            } else {
                                format!("edit {}", path)
                            }
                        }
                        "list_dir" => {
                            let args: serde_json::Value =
                                serde_json::from_str(arguments).unwrap_or_default();
                            let path = args["path"].as_str().unwrap_or(".");
                            format!("ls {}", path)
                        }
                        _ => name.to_string(),
                    };
                    self.add_message("tool", &display, None);
                }
            }
            AppEvent::ApprovalPending {
                preview,
                is_tty_command,
                command,
            } => {
                self.approval_selected = ApprovalChoice::Allow;
                self.feedback_clear();
                self.messages.push(DisplayMessage {
                    role: "diff".to_string(),
                    content: preview.clone(),
                    thinking: None,
                    collapsed: false,
                    hidden: false,
                });
                self.scroll_to_bottom();
                self.mode = AppMode::Approval {
                    preview: String::new(),
                    is_tty_command: *is_tty_command,
                    command: command.clone(),
                };
            }
            AppEvent::CommandResult { output } => {
                let trimmed = output.trim().to_string();
                let collapsed = trimmed.lines().count() > 5;
                self.messages.push(DisplayMessage {
                    role: "system".to_string(),
                    content: trimmed,
                    thinking: None,
                    collapsed,
                    hidden: false,
                });
            }
            AppEvent::CommandRejected { feedback } => {
                self.add_message("system", feedback, None);
            }
            AppEvent::Ready => {}
            AppEvent::Error { message } => {
                self.add_message("system", &format!("Error: {}", message), None);
            }
            AppEvent::ToolCallsStart => {}
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            messages: Vec::new(),
            thinking_buffer: String::new(),
            text_buffer: String::new(),
            mode: AppMode::Chat,
            input: String::new(),
            input_cursor_char: 0,
            should_quit: false,
            thinking_visible: true,
            verbosity: Verbosity::Normal,
            is_streaming: false,
            queued_messages: VecDeque::new(),
            scroll_offset: TAIL_SENTINEL,
            pending_scroll_delta: 0,
            approval_selected: ApprovalChoice::Allow,
            approval_feedback: String::new(),
            approval_feedback_cursor: 0,
            slash_filter: String::new(),
            slash_selected: 0,
            slash_matched: Vec::new(),
        }
    }
}

// ── Drawing ──

pub fn draw(f: &mut Frame, state: &mut AppState) {
    let bottom_height = match &state.mode {
        AppMode::Approval { .. } => {
            let feedback_bonus =
                if state.approval_selected == ApprovalChoice::EditFeedback { 3 } else { 0 };
            1 + feedback_bonus + 3
        }
        _ => 3,
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(bottom_height)])
        .split(f.area());

    draw_messages(f, state, chunks[0], chunks[0].width as usize);

    match &state.mode {
        AppMode::Approval { .. } => {
            let has_feedback = state.approval_selected == ApprovalChoice::EditFeedback;
            let mut constraints = vec![Constraint::Length(1)];
            if has_feedback {
                constraints.push(Constraint::Length(3));
            }
            constraints.push(Constraint::Length(3));

            let approval_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints(constraints)
                .split(chunks[1]);

            draw_approval_choices(f, state, approval_chunks[0]);
            let input_idx = if has_feedback {
                draw_feedback_input(f, state, approval_chunks[1]);
                2
            } else {
                1
            };
            draw_input(f, state, approval_chunks[input_idx]);
        }
        _ => {
            draw_input(f, state, chunks[1]);
        }
    }

    if state.mode == AppMode::SlashMenu {
        draw_slash_menu(f, state, f.area());
    }

    if !state.queued_messages.is_empty() {
        let queue_height = 1;
        let queue_area = Rect {
            x: f.area().x,
            y: f.area().bottom().saturating_sub(4),
            width: f.area().width,
            height: queue_height,
        };
        let label = if state.queued_messages.len() == 1 {
            "1 queued message".to_string()
        } else {
            format!("{} queued messages", state.queued_messages.len())
        };
        let queue_para = Paragraph::new(Line::from(Span::styled(
            format!("  \u{25B8} {} (Enter to edit, Esc to cancel)", label),
            Style::default().fg(Color::Yellow),
        )));
        f.render_widget(Clear, queue_area);
        f.render_widget(queue_para, queue_area);
    }
}

fn draw_messages(f: &mut Frame, state: &mut AppState, area: Rect, terminal_width: usize) {
    let text = build_all_lines(state, terminal_width);
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

    let paragraph = Paragraph::new(padded);
    f.render_widget(paragraph, area);

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
            .track_style(Style::default().fg(Color::DarkGray))
            .thumb_symbol("\u{2503}")
            .thumb_style(Style::default().fg(Color::White))
            .render(area, f.buffer_mut(), &mut sb_state);
    }
}

fn draw_input(f: &mut Frame, state: &AppState, area: Rect) {
    let is_approval = matches!(state.mode, AppMode::Approval { .. });
    let border_color = if is_approval {
        Color::Yellow
    } else if state.is_streaming {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let prompt_width = UnicodeWidthStr::width(PROMPT) as u16;

    let input_spans = if state.is_streaming && !state.input.is_empty() {
        vec![
            Span::styled(PROMPT, Style::default().fg(COLOR_PROMPT).add_modifier(Modifier::BOLD)),
            Span::styled(
                state.input.as_str(),
                Style::default().fg(Color::DarkGray),
            ),
        ]
    } else {
        vec![
            Span::styled(PROMPT, Style::default().fg(COLOR_PROMPT).add_modifier(Modifier::BOLD)),
            Span::styled(state.input.as_str(), Style::default().fg(COLOR_INPUT)),
        ]
    };

    let content = Line::from(input_spans);
    let input = Paragraph::new(content).block(
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(border_color)),
    );
    f.render_widget(input, area);

    let byte_pos = state
        .input
        .char_indices()
        .nth(state.input_cursor_char)
        .map(|(i, _)| i)
        .unwrap_or(state.input.len());
    let display_col: u16 = UnicodeWidthStr::width(&state.input[..byte_pos]) as u16;
    f.set_cursor_position((area.x + prompt_width + display_col, area.y + 1));
}

fn draw_approval_choices(f: &mut Frame, state: &AppState, area: Rect) {
    let selected = state.approval_selected;
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
            vec![Span::styled(styled, choice.style(sel))]
        })
        .collect();
    let choices_line = Paragraph::new(Line::from(choices));
    f.render_widget(choices_line, area);
}

fn draw_feedback_input(f: &mut Frame, state: &AppState, area: Rect) {
    let feedback_block = Block::default()
        .borders(Borders::ALL)
        .title(" Feedback ")
        .border_style(Style::default().fg(COLOR_TOOL));
    let feedback_para = Paragraph::new(Span::styled(
        state.approval_feedback.as_str(),
        Style::default().fg(COLOR_INPUT),
    ))
    .block(feedback_block);
    f.render_widget(feedback_para, area);

    let fb_byte_pos = state
        .approval_feedback
        .char_indices()
        .nth(state.approval_feedback_cursor)
        .map(|(i, _)| i)
        .unwrap_or(state.approval_feedback.len());
    let fb_col: u16 = UnicodeWidthStr::width(&state.approval_feedback[..fb_byte_pos]) as u16;
    f.set_cursor_position((area.x + 1 + fb_col, area.y + 1));
}

fn draw_slash_menu(f: &mut Frame, state: &AppState, area: Rect) {
    if state.slash_matched.is_empty() {
        return;
    }

    let menu_width = 40u16;
    let menu_height = (state.slash_matched.len().min(8) as u16) + 2;
    let menu_area = Rect {
        x: area.x + 2,
        y: area.bottom().saturating_sub(3 + menu_height as u16),
        width: menu_width.min(area.width),
        height: menu_height,
    };

    f.render_widget(Clear, menu_area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(COLOR_TOOL));
    f.render_widget(block, menu_area);

    let inner = Rect {
        x: menu_area.x + 1,
        y: menu_area.y + 1,
        width: menu_area.width.saturating_sub(2),
        height: menu_area.height.saturating_sub(2),
    };

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
            if sel {
                Line::from(vec![
                    Span::styled(
                        format!(" \u{25B8} {}", name_str),
                        Style::default().fg(Color::Black).bg(COLOR_TOOL).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        " ".repeat(inner.width as usize / 2),
                        Style::default().bg(COLOR_TOOL),
                    ),
                ])
            } else {
                Line::from(Span::styled(
                    format!("   {}", name_str),
                    Style::default().fg(COLOR_TOOL),
                ))
            }
        })
        .collect();

    let para = Paragraph::new(items);
    f.render_widget(para, inner);
}
