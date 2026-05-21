use std::collections::VecDeque;

use ratatui::layout::Rect;

use super::input::InputBuffer;
use super::theme::Theme;
use super::types::*;

// ── Sub-structs ──────────────────────────────────────

pub struct FatigueState {
    pub fatigue: f64,
    pub turns: u32,
    pub tool_calls: u32,
    pub desc: String,
}

impl Default for FatigueState {
    fn default() -> Self {
        Self {
            fatigue: 0.0,
            turns: 0,
            tool_calls: 0,
            desc: "精神饱满。".to_string(),
        }
    }
}

pub struct DisplayConfig {
    pub thinking_visible: bool,
    pub verbosity: Verbosity,
    pub theme: Theme,
}

pub struct ScrollState {
    pub offset: usize,
    pub pending_delta: i32,
}

pub struct ViewportState {
    pub clickable_ranges: Vec<ClickRange>,
    pub hovered_msg_index: Option<usize>,
    pub content_top: usize,
    pub content_height: usize,
    pub messages_area: Rect,
    pub cached_line_texts: Vec<String>,
}

pub struct MouseState {
    pub selection: Option<(usize, usize, usize, usize)>,
    pub drag_scroll_pos: Option<(u16, u16)>,
    pub mouse_down_pos: Option<(usize, usize)>,
    pub mouse_down_on_toggle: Option<(usize, usize)>,
}

// ── Scroll methods ───────────────────────────────────

const TAIL_SENTINEL: usize = usize::MAX;

impl ScrollState {
    pub fn is_at_tail(&self) -> bool {
        self.offset == TAIL_SENTINEL
    }

    pub fn resolve_top(&self, max_start: usize) -> usize {
        if self.offset == TAIL_SENTINEL {
            max_start
        } else {
            self.offset.min(max_start)
        }
    }

    pub fn up(&mut self, n: usize) {
        self.pending_delta -= n as i32;
    }

    pub fn down(&mut self, n: usize) {
        self.pending_delta += n as i32;
    }

    pub fn to_bottom(&mut self) {
        self.offset = TAIL_SENTINEL;
        self.pending_delta = 0;
    }

    pub fn apply_pending(&mut self, total_lines: usize, visible_lines: usize) {
        let delta = self.pending_delta;
        if delta == 0 {
            return;
        }
        self.pending_delta = 0;

        if total_lines <= visible_lines {
            self.offset = TAIL_SENTINEL;
            return;
        }

        let max_start = total_lines.saturating_sub(visible_lines);
        let current = if self.offset == TAIL_SENTINEL {
            max_start
        } else {
            self.offset.min(max_start)
        };

        let new_top = if delta < 0 {
            current.saturating_sub(delta.unsigned_abs() as usize)
        } else {
            current.saturating_add(delta as usize).min(max_start)
        };

        self.offset = if new_top >= max_start {
            TAIL_SENTINEL
        } else {
            new_top
        };
    }
}

// ── AppState ─────────────────────────────────────────

pub struct AppState {
    // Core content
    pub messages: Vec<DisplayMessage>,
    pub thinking_buffer: String,
    pub text_buffer: String,
    pub input: InputBuffer,
    pub queued_messages: VecDeque<String>,

    // Mode (carries mode-specific state)
    pub mode: crate::tui::mode::AppMode,
    pub is_streaming: bool,
    pub should_quit: bool,
    pub model_name: String,
    pub conversation_path: String,

    // Grouped state
    pub display: DisplayConfig,
    pub scroll: ScrollState,
    pub viewport: ViewportState,
    pub mouse: MouseState,
    pub fatigue: FatigueState,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            messages: Vec::new(),
            thinking_buffer: String::new(),
            text_buffer: String::new(),
            input: InputBuffer::new(),
            queued_messages: VecDeque::new(),
            mode: crate::tui::mode::AppMode::Welcome,
            is_streaming: false,
            should_quit: false,
            model_name: String::new(),
            conversation_path: String::new(),
            display: DisplayConfig {
                thinking_visible: true,
                verbosity: Verbosity::Normal,
                theme: Theme::default(),
            },
            scroll: ScrollState {
                offset: usize::MAX,
                pending_delta: 0,
            },
            viewport: ViewportState {
                clickable_ranges: Vec::new(),
                hovered_msg_index: None,
                content_top: 0,
                content_height: 0,
                messages_area: Rect::default(),
                cached_line_texts: Vec::new(),
            },
            mouse: MouseState {
                selection: None,
                drag_scroll_pos: None,
                mouse_down_pos: None,
                mouse_down_on_toggle: None,
            },
            fatigue: FatigueState::default(),
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

    pub fn with_conversation_path(mut self, path: String) -> Self {
        self.conversation_path = path;
        self
    }

    pub fn load_conversation(&mut self) {
        use crate::agent::history::History;
        use crate::tui::types::DisplayMessage;

        let path = std::path::PathBuf::from(&self.conversation_path);
        if !path.exists() {
            self.mode = crate::tui::mode::AppMode::Chat;
            return;
        }

        match History::load(path) {
            Ok(history) => {
                for msg in history.messages() {
                    match msg.role.as_str() {
                        "user" => {
                            let content = msg.content.as_deref().unwrap_or("");
                            if content.is_empty() {
                                continue;
                            }
                            self.messages.push(DisplayMessage::Chat {
                                role: "user".to_string(),
                                content: content.to_string(),
                                thinking: None,
                                collapsed: false,
                                hidden: false,
                            });
                        }
                        "assistant" => {
                            let content = msg.content.as_deref().unwrap_or("");
                            if content.is_empty() && msg.tool_calls.is_none() {
                                continue;
                            }
                            if !content.is_empty() {
                                self.messages.push(DisplayMessage::Chat {
                                    role: "assistant".to_string(),
                                    content: content.to_string(),
                                    thinking: msg.reasoning_content.clone(),
                                    collapsed: msg.reasoning_content.is_some(),
                                    hidden: false,
                                });
                            }
                            if let Some(tool_calls) = &msg.tool_calls {
                                for tc in tool_calls {
                                    let display = format!(
                                        "{}({})",
                                        tc.function.name,
                                        truncate_args(&tc.function.arguments)
                                    );
                                    self.messages.push(DisplayMessage::Tool {
                                        call_id: tc.id.clone(),
                                        name: tc.function.name.clone(),
                                        args_display: display,
                                        result: None,
                                        diff_preview: None,
                                        collapsed: true,
                                        hidden: false,
                                    });
                                }
                            }
                        }
                        "tool" => {
                            if let Some(DisplayMessage::Tool { result, .. }) = self
                                .messages
                                .iter_mut()
                                .rev()
                                .find(|m| matches!(m, DisplayMessage::Tool { call_id, .. } if *call_id == msg.tool_call_id.as_deref().unwrap_or("")))
                            {
                                *result = Some(
                                    msg.content
                                        .as_deref()
                                        .unwrap_or("")
                                        .chars()
                                        .take(200)
                                        .collect(),
                                );
                            }
                        }
                        _ => {}
                    }
                }
                self.scroll.to_bottom();
            }
            Err(e) => {
                tracing::warn!("Failed to load conversation: {}", e);
            }
        }
        self.mode = crate::tui::mode::AppMode::Chat;
    }

    pub fn theme(&self) -> &Theme {
        &self.display.theme
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
            diff_preview: None,
            collapsed: true,
            hidden: false,
        });
    }

    pub fn complete_tool(
        &mut self,
        call_id: &str,
        result: String,
        diff_preview: Option<String>,
        force_collapsed: bool,
    ) {
        if let Some(DisplayMessage::Tool {
            result: r,
            diff_preview: dp,
            collapsed,
            hidden,
            ..
        }) = find_tool_by_call_id_mut(&mut self.messages, call_id)
        {
            *r = Some(result);
            *dp = diff_preview;
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

    pub fn should_show_tool_result(&self, tool_name: &str) -> bool {
        match self.display.verbosity {
            Verbosity::Trace => true,
            Verbosity::Verbose => !QUIET_TOOLS.contains(&tool_name),
            Verbosity::Normal => false,
        }
    }

    pub fn format_tool_args(&self, name: &str, arguments: &str) -> String {
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
        if let crate::tui::mode::AppMode::SlashMenu(ref mut sm) = self.mode {
            sm.update_matches(self.input.as_str());
        }
    }

    pub fn slash_selected_cmd(&self) -> Option<&SlashCommand> {
        match &self.mode {
            crate::tui::mode::AppMode::SlashMenu(sm) => sm.selected_cmd(),
            _ => None,
        }
    }
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

fn truncate_args(args: &str) -> String {
    let s: String = args.chars().take(60).collect();
    if args.len() > 60 {
        format!("{}...", s)
    } else {
        s
    }
}

const QUIET_TOOLS: &[&str] = &[
    "read_file",
    "list_dir",
    "edit_soul",
    "edit_memory",
    "edit_profile",
];
