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
use unicode_width::UnicodeWidthChar;
use unicode_width::UnicodeWidthStr;

use super::render::build_all_lines;
use super::theme::{APP_NAME, APP_TAGLINE, PROMPT, Theme, logo_lines};
use crate::agent::events::AppEvent;

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

    fn style(self, selected: bool, theme: &Theme) -> Style {
        if selected {
            match self {
                ApprovalChoice::Allow => Style::default()
                    .fg(Color::Black)
                    .bg(theme.success)
                    .add_modifier(Modifier::BOLD),
                ApprovalChoice::Reject => Style::default()
                    .fg(Color::Black)
                    .bg(theme.error)
                    .add_modifier(Modifier::BOLD),
                ApprovalChoice::EditFeedback => Style::default()
                    .fg(Color::Black)
                    .bg(theme.warning)
                    .add_modifier(Modifier::BOLD),
            }
        } else {
            Style::default().fg(theme.muted)
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
    SlashCommand {
        name: "help",
        aliases: &["?"],
        description: "Show available commands",
    },
    SlashCommand {
        name: "clear",
        aliases: &[],
        description: "Clear conversation history",
    },
    SlashCommand {
        name: "quit",
        aliases: &["q"],
        description: "Quit the application",
    },
    SlashCommand {
        name: "verbose",
        aliases: &["v"],
        description: "Cycle display verbosity",
    },
    SlashCommand {
        name: "thinking",
        aliases: &["think"],
        description: "Toggle thinking display",
    },
    SlashCommand {
        name: "model",
        aliases: &[],
        description: "Show current model",
    },
    SlashCommand {
        name: "history",
        aliases: &["h"],
        description: "Show recent history",
    },
];

#[derive(Debug, Clone)]
pub enum DisplayMessage {
    Chat {
        role: String,
        content: String,
        thinking: Option<String>,
        collapsed: bool,
        hidden: bool,
    },
    Tool {
        call_id: String,
        name: String,
        args_display: String,
        result: Option<String>,
        collapsed: bool,
        hidden: bool,
    },
    Diff {
        content: String,
        hidden: bool,
    },
}

impl DisplayMessage {
    pub fn is_hidden(&self) -> bool {
        match self {
            DisplayMessage::Chat { hidden, .. }
            | DisplayMessage::Tool { hidden, .. }
            | DisplayMessage::Diff { hidden, .. } => *hidden,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AppMode {
    Chat,
    Approval {
        tool_call_id: String,
        tool_name: String,
        tool_label: String,
    },
    Streaming,
    SlashMenu,
}

pub struct ClickRange {
    pub line: usize,
    pub col_start: usize,
    pub col_end: usize,
    pub msg_index: usize,
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

    pub model_name: String,
    theme: Theme,

    pub clickable_ranges: Vec<ClickRange>,
    pub hovered_msg_index: Option<usize>,
    pub content_top: usize,
    pub content_height: usize,
    pub messages_area: Rect,

    pub selection: Option<(usize, usize, usize, usize)>,
    pub cached_line_texts: Vec<String>,
    pub drag_scroll_pos: Option<(u16, u16)>,
    mouse_down_pos: Option<(usize, usize)>,
    mouse_down_on_toggle: Option<(usize, usize)>,
}

fn char_to_byte(char_idx: usize, s: &str) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(i, _)| i)
        .unwrap_or(s.len())
}

fn display_width_to_char_idx(width: usize, s: &str) -> usize {
    let mut w = 0;
    for (ci, c) in s.char_indices() {
        if w >= width {
            return ci;
        }
        w += c.width().unwrap_or(0);
    }
    s.len()
}

fn insert_char(buf: &mut String, cursor: &mut usize, c: char) {
    let pos = char_to_byte(*cursor, buf);
    buf.insert(pos, c);
    *cursor += 1;
}

fn delete_before(buf: &mut String, cursor: &mut usize) {
    if *cursor == 0 {
        return;
    }
    *cursor -= 1;
    let pos = char_to_byte(*cursor, buf);
    let len = buf[pos..].chars().next().map(|c| c.len_utf8()).unwrap_or(0);
    buf.drain(pos..pos + len);
}

fn delete_after(buf: &mut String, cursor: &mut usize) {
    let total = buf.chars().count();
    if *cursor >= total {
        return;
    }
    let pos = char_to_byte(*cursor, buf);
    let len = buf[pos..].chars().next().map(|c| c.len_utf8()).unwrap_or(0);
    buf.drain(pos..pos + len);
}

fn find_tool_by_call_id_mut<'a>(
    messages: &'a mut [DisplayMessage],
    call_id: &str,
) -> Option<&'a mut DisplayMessage> {
    messages.iter_mut().rev().find(|m| match m {
        DisplayMessage::Tool { call_id: cid, .. } => cid == call_id,
        _ => false,
    })
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
            model_name: String::new(),
            theme: Theme::default(),
            clickable_ranges: Vec::new(),
            hovered_msg_index: None,
            content_top: 0,
            content_height: 0,
            messages_area: Rect::default(),
            selection: None,
            cached_line_texts: Vec::new(),
            drag_scroll_pos: None,
            mouse_down_pos: None,
            mouse_down_on_toggle: None,
        }
    }
}

impl AppState {
    pub fn new(model_name: String) -> Self {
        Self {
            model_name,
            ..Self::default()
        }
    }

    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    pub fn add_chat(&mut self, role: &str, content: &str, thinking: Option<String>) {
        self.messages.push(DisplayMessage::Chat {
            role: role.to_string(),
            content: content.to_string(),
            thinking,
            collapsed: false,
            hidden: false,
        });
    }

    pub fn add_tool_call(&mut self, call_id: String, name: String, args_display: String) {
        self.messages.push(DisplayMessage::Tool {
            call_id,
            name,
            args_display,
            result: None,
            collapsed: true,
            hidden: false,
        });
    }

    pub fn complete_tool(&mut self, call_id: &str, result: String, force_collapsed: bool) {
        if let Some(DisplayMessage::Tool {
            result: r,
            collapsed,
            hidden,
            ..
        }) = find_tool_by_call_id_mut(&mut self.messages, call_id)
        {
            *r = Some(result);
            *collapsed = force_collapsed;
            *hidden = false;
        }
    }

    pub fn hide_tool(&mut self, call_id: &str) {
        if let Some(DisplayMessage::Tool { hidden, .. }) =
            find_tool_by_call_id_mut(&mut self.messages, call_id)
        {
            *hidden = true;
        }
    }

    pub fn add_diff(&mut self, content: String) {
        self.messages.push(DisplayMessage::Diff {
            content,
            hidden: false,
        });
    }

    fn should_show_tool_result(&self, tool_name: &str) -> bool {
        match self.verbosity {
            Verbosity::Trace => true,
            Verbosity::Verbose => !QUIET_TOOLS.contains(&tool_name),
            Verbosity::Normal => false,
        }
    }

    pub fn input_insert(&mut self, c: char) {
        insert_char(&mut self.input, &mut self.input_cursor_char, c);
    }

    pub fn input_backspace(&mut self) {
        delete_before(&mut self.input, &mut self.input_cursor_char);
    }

    pub fn input_delete(&mut self) {
        delete_after(&mut self.input, &mut self.input_cursor_char);
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

    pub fn feedback_insert(&mut self, c: char) {
        insert_char(
            &mut self.approval_feedback,
            &mut self.approval_feedback_cursor,
            c,
        );
    }

    pub fn feedback_backspace(&mut self) {
        delete_before(
            &mut self.approval_feedback,
            &mut self.approval_feedback_cursor,
        );
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

    fn format_tool_args(&self, name: &str, arguments: &str) -> String {
        match name {
            "run_command" => {
                let args: serde_json::Value = serde_json::from_str(arguments).unwrap_or_default();
                let cmd = args["command"].as_str().unwrap_or("?");
                format!("$ {}", cmd)
            }
            "read_file" => {
                let args: serde_json::Value = serde_json::from_str(arguments).unwrap_or_default();
                let path = args["path"].as_str().unwrap_or("?");
                format!("read {}", path)
            }
            "write_file" => {
                let args: serde_json::Value = serde_json::from_str(arguments).unwrap_or_default();
                let path = args["path"].as_str().unwrap_or("?");
                format!("write {}", path)
            }
            "edit_file" => {
                let args: serde_json::Value = serde_json::from_str(arguments).unwrap_or_default();
                let path = args["path"].as_str().unwrap_or("?");
                if let Some(count) = args["count"].as_u64() {
                    format!("edit {} ({} changes)", path, count)
                } else {
                    format!("edit {}", path)
                }
            }
            "list_dir" => {
                let args: serde_json::Value = serde_json::from_str(arguments).unwrap_or_default();
                let path = args["path"].as_str().unwrap_or(".");
                format!("ls {}", path)
            }
            _ => name.to_string(),
        }
    }

    pub fn handle_mouse_down(&mut self, row: u16, col: u16) {
        let abs = self.screen_to_content(row);
        let (abs, col) = match abs {
            Some(v) => (v, col as usize),
            None => return,
        };
        self.mouse_down_pos = Some((abs, col));
        self.mouse_down_on_toggle = None;

        for range in self.clickable_ranges.iter() {
            if abs == range.line && col >= range.col_start && col < range.col_end {
                self.mouse_down_on_toggle = Some((abs, col));
                return;
            }
        }
    }

    pub fn handle_mouse_drag(&mut self, row: u16, col: u16) {
        if self.mouse_down_on_toggle.is_some() {
            self.mouse_down_on_toggle = None;
        }

        let Some((start_line, start_col)) = self.mouse_down_pos else {
            return;
        };

        let area = self.messages_area;
        let col_usize = col as usize;
        let at_edge = row <= area.y || row >= area.y + area.height;

        // Scroll at edges during active selection
        if self.selection.is_some() && at_edge {
            if row <= area.y {
                self.scroll_up(1);
            } else {
                self.scroll_down(1);
            }
            self.drag_scroll_pos = Some((row, col));
        } else if !at_edge {
            self.drag_scroll_pos = None;
        }

        // Clamp to viewport and compute content position
        let clamped_row = row.clamp(area.y, area.y + area.height.saturating_sub(1));
        let abs = match self.screen_to_content(clamped_row) {
            Some(v) => v,
            None => return,
        };

        if self.selection.is_none() && (abs != start_line || col_usize.abs_diff(start_col) > 2) {
            self.selection = Some((start_line, start_col, abs, col_usize));
        } else if self.selection.is_some() {
            let (s_line, s_col, _, _) = self.selection.unwrap();
            self.selection = Some((s_line, s_col, abs, col_usize));
        }
    }

    pub fn handle_mouse_up(&mut self) -> Option<String> {
        if let Some((_line, _col)) = self.mouse_down_on_toggle.take() {
            for range in self.clickable_ranges.iter() {
                if _line == range.line && _col >= range.col_start && _col < range.col_end {
                    self.toggle_by_index(range.msg_index);
                    self.hovered_msg_index = None;
                    break;
                }
            }
            return None;
        }

        let copied = if let Some(sel) = self.selection.take() {
            let (sl, sc, el, ec) = sel;
            if sl != el || sc != ec {
                Some(self.extract_selected_text(sl, sc, el, ec))
            } else {
                None
            }
        } else {
            None
        };

        self.mouse_down_pos = None;
        self.mouse_down_on_toggle = None;
        self.drag_scroll_pos = None;
        copied
    }

    fn extract_selected_text(
        &self,
        start_line: usize,
        start_col: usize,
        end_line: usize,
        end_col: usize,
    ) -> String {
        let (sl, sc, el, ec) =
            if start_line < end_line || (start_line == end_line && start_col <= end_col) {
                (start_line, start_col, end_line, end_col)
            } else {
                (end_line, end_col, start_line, start_col)
            };

        let mut result = String::new();
        for line_idx in sl..=el {
            if line_idx >= self.cached_line_texts.len() {
                break;
            }
            let text = &self.cached_line_texts[line_idx];
            if line_idx == sl && line_idx == el {
                let sb = display_width_to_char_idx(sc, text);
                let eb = display_width_to_char_idx(ec, text);
                if eb > sb {
                    result.push_str(&text[sb..eb]);
                }
            } else if line_idx == sl {
                let sb = display_width_to_char_idx(sc, text);
                result.push_str(&text[sb..]);
                result.push('\n');
            } else if line_idx == el {
                let eb = display_width_to_char_idx(ec, text);
                result.push_str(&text[..eb]);
            } else {
                result.push_str(text);
                result.push('\n');
            }
        }
        result
    }

    pub fn handle_hover(&mut self, row: u16, col: u16) -> bool {
        let abs = self.screen_to_content(row);
        let abs = match abs {
            Some(v) => v,
            None => {
                if self.hovered_msg_index.is_some() {
                    self.hovered_msg_index = None;
                }
                return false;
            }
        };
        let col = col as usize;
        for range in self.clickable_ranges.iter() {
            if abs == range.line && col >= range.col_start && col < range.col_end {
                if self.hovered_msg_index != Some(range.msg_index) {
                    self.hovered_msg_index = Some(range.msg_index);
                }
                return true;
            }
        }
        if self.hovered_msg_index.is_some() {
            self.hovered_msg_index = None;
        }
        false
    }

    fn screen_to_content(&self, row: u16) -> Option<usize> {
        let area = self.messages_area;
        if row < area.y || row >= area.y + area.height {
            return None;
        }
        let visible_row = (row - area.y) as usize;
        let visible_height = area.height as usize;
        let padding = if self.content_height < visible_height {
            visible_height - self.content_height
        } else {
            0
        };
        if visible_row < padding {
            return None;
        }
        Some(self.content_top + visible_row - padding)
    }

    fn toggle_by_index(&mut self, idx: usize) {
        if let Some(msg) = self.messages.get_mut(idx) {
            match msg {
                DisplayMessage::Chat { collapsed, .. } | DisplayMessage::Tool { collapsed, .. } => {
                    *collapsed = !*collapsed;
                }
                DisplayMessage::Diff { .. } => {}
            }
        }
    }

    pub fn toggle_last_collapsed(&mut self) {
        for msg in self.messages.iter_mut().rev() {
            match msg {
                DisplayMessage::Chat { collapsed, .. } | DisplayMessage::Tool { collapsed, .. } => {
                    if *collapsed {
                        *collapsed = false;
                        return;
                    }
                }
                DisplayMessage::Diff { .. } => continue,
            }
        }
    }

    pub fn update_slash_matches(&mut self) {
        let filter = if self.input.starts_with('/') {
            &self.input[1..]
        } else {
            ""
        };
        self.slash_filter = filter.to_string();
        self.slash_matched.clear();
        for (i, cmd) in BUILTIN_COMMANDS.iter().enumerate() {
            if cmd.name.starts_with(filter) || cmd.aliases.iter().any(|a| a.starts_with(filter)) {
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
                self.add_chat("assistant", full_text, thinking);
                self.text_buffer.clear();
            }
            AppEvent::TurnComplete => {
                self.is_streaming = false;
                self.mode = AppMode::Chat;
                if let Some(msg) = self.queued_messages.pop_front() {
                    self.input_clear();
                    self.add_chat("user", &msg, None);
                }
            }
            AppEvent::ToolResult {
                call_id,
                name,
                result,
            } => {
                let show = self.should_show_tool_result(name);
                self.complete_tool(call_id, result.clone(), !show);
            }
            AppEvent::ToolCalling {
                call_id,
                name,
                arguments,
            } => {
                if !self.text_buffer.is_empty() {
                    let thinking = if self.thinking_buffer.is_empty() {
                        None
                    } else {
                        Some(std::mem::take(&mut self.thinking_buffer))
                    };
                    let text = std::mem::take(&mut self.text_buffer);
                    self.add_chat("assistant", &text, thinking);
                }
                let display = self.format_tool_args(name, arguments);
                self.add_tool_call(call_id.clone(), name.clone(), display);
            }
            AppEvent::ApprovalPending {
                call_id,
                tool_name,
                preview,
            } => {
                self.approval_selected = ApprovalChoice::Allow;
                self.feedback_clear();

                let first_line = preview.lines().next().unwrap_or("");

                let tool_label = if first_line.starts_with("$ ") {
                    format!("→ {}", first_line)
                } else if first_line.starts_with("---") || first_line.starts_with("diff ") {
                    let second = preview.lines().nth(1).unwrap_or("");
                    if let Some(path) = second.strip_prefix("+++ ") {
                        format!(
                            "→ edit {}",
                            path.trim_start_matches('b').trim_start_matches('/')
                        )
                    } else {
                        "→ edit".to_string()
                    }
                } else {
                    format!("→ {}", first_line)
                };

                let has_diff_content = preview.lines().count() > 1 || !preview.starts_with("$ ");

                if has_diff_content {
                    if let Some(last) = self.messages.last_mut() {
                        if let DisplayMessage::Tool { hidden, .. } = last {
                            *hidden = true;
                        }
                    }

                    self.add_diff(preview.clone());
                }
                self.scroll_to_bottom();
                self.mode = AppMode::Approval {
                    tool_call_id: call_id.clone(),
                    tool_name: tool_name.clone(),
                    tool_label,
                };
            }
            AppEvent::ToolRejected { call_id, feedback } => {
                self.complete_tool(call_id, feedback.clone(), true);
            }
            AppEvent::Error { message } => {
                self.add_chat("system", &format!("Error: {}", message), None);
            }
        }
    }
}

pub fn draw(f: &mut Frame, state: &mut AppState) {
    let theme = state.theme().clone();
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
        draw_messages(f, state, chunks[0], chunks[0].width.saturating_sub(1) as usize);
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
            format!(
                "\u{276F} {} queued \u{2190} ",
                state.queued_messages.len()
            ),
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

    let byte_pos = state
        .input
        .char_indices()
        .nth(state.input_cursor_char)
        .map(|(i, _)| i)
        .unwrap_or(state.input.len());
    let display_col: u16 = UnicodeWidthStr::width(&state.input[..byte_pos]) as u16;
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
    let label_w = max_w.saturating_sub(3); // "   " prefix
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
        state.approval_feedback.as_str(),
        Style::default().fg(theme.input),
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
                            .fg(Color::Black)
                            .bg(theme.tool)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        desc_display,
                        Style::default().fg(Color::Black).bg(theme.tool),
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
