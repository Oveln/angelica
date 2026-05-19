use std::collections::VecDeque;

use ratatui::layout::Rect;

use super::input::InputBuffer;
use super::theme::Theme;
use super::types::*;

const QUIET_TOOLS: &[&str] = &[
    "read_file",
    "list_dir",
    "query_sessions",
    "update_agent_memory",
    "update_user_profile",
    "update_soul",
];

fn find_tool_by_call_id_mut<'a>(
    messages: &'a mut [DisplayMessage],
    call_id: &str,
) -> Option<&'a mut DisplayMessage> {
    messages.iter_mut().rev().find(|m| match m {
        DisplayMessage::Tool { call_id: cid, .. } => cid == call_id,
        _ => false,
    })
}

pub struct AppState {
    pub messages: Vec<DisplayMessage>,
    pub thinking_buffer: String,
    pub text_buffer: String,
    pub mode: AppMode,
    pub input: InputBuffer,
    pub should_quit: bool,
    pub thinking_visible: bool,
    pub verbosity: Verbosity,
    pub is_streaming: bool,

    pub queued_messages: VecDeque<String>,

    pub scroll_offset: usize,
    pub pending_scroll_delta: i32,

    pub approval_selected: ApprovalChoice,
    pub feedback: InputBuffer,

    pub slash_filter: String,
    pub slash_selected: usize,
    pub slash_matched: Vec<usize>,

    pub model_name: String,
    pub theme: Theme,

    pub clickable_ranges: Vec<ClickRange>,
    pub hovered_msg_index: Option<usize>,
    pub content_top: usize,
    pub content_height: usize,
    pub messages_area: Rect,

    pub selection: Option<(usize, usize, usize, usize)>,
    pub cached_line_texts: Vec<String>,
    pub drag_scroll_pos: Option<(u16, u16)>,
    pub mouse_down_pos: Option<(usize, usize)>,
    pub mouse_down_on_toggle: Option<(usize, usize)>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            messages: Vec::new(),
            thinking_buffer: String::new(),
            text_buffer: String::new(),
            mode: AppMode::Chat,
            input: InputBuffer::new(),
            should_quit: false,
            thinking_visible: true,
            verbosity: Verbosity::Normal,
            is_streaming: false,
            queued_messages: VecDeque::new(),
            scroll_offset: usize::MAX,
            pending_scroll_delta: 0,
            approval_selected: ApprovalChoice::Allow,
            feedback: InputBuffer::new(),
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

    pub fn should_show_tool_result(&self, tool_name: &str) -> bool {
        match self.verbosity {
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
}
