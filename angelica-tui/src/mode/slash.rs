use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use tokio::sync::mpsc;

use super::AppMode;
use crate::state::AppState;
use crate::theme::Theme;
use crate::types::{BUILTIN_COMMANDS, SlashCommand};

pub enum SlashAction {
    None,
    ExecuteCommand(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SlashMenuState {
    pub filter: String,
    pub selected: usize,
    pub matched: Vec<usize>,
}

impl SlashMenuState {
    pub fn new() -> Self {
        Self {
            filter: String::new(),
            selected: 0,
            matched: Vec::new(),
        }
    }

    pub fn update_matches(&mut self, input: &str) {
        let filter = input.strip_prefix('/').unwrap_or("");
        self.filter = filter.to_string();
        self.matched.clear();
        for (i, cmd) in BUILTIN_COMMANDS.iter().enumerate() {
            if cmd.name.starts_with(filter) || cmd.aliases.iter().any(|a| a.starts_with(filter)) {
                self.matched.push(i);
            }
        }
        if self.selected >= self.matched.len() {
            self.selected = 0;
        }
    }

    pub fn selected_cmd(&self) -> Option<&'static SlashCommand> {
        self.matched
            .get(self.selected)
            .and_then(|&i| BUILTIN_COMMANDS.get(i))
    }
}

impl Default for SlashMenuState {
    fn default() -> Self {
        Self::new()
    }
}

pub async fn handle_key(
    state: &mut AppState,
    key: KeyEvent,
    _tx: &mpsc::Sender<angelica::agent::events::UserAction>,
) -> SlashAction {
    match key.code {
        KeyCode::Esc => {
            state.input.clear();
            state.mode = if state.is_streaming {
                AppMode::Streaming
            } else {
                AppMode::Chat
            };
        }
        KeyCode::Up => {
            if let AppMode::SlashMenu(ref mut sm) = state.mode
                && sm.selected > 0
            {
                sm.selected -= 1;
            }
        }
        KeyCode::Down => {
            if let AppMode::SlashMenu(ref mut sm) = state.mode
                && sm.selected + 1 < sm.matched.len()
            {
                sm.selected += 1;
            }
        }
        KeyCode::Tab => {
            if let AppMode::SlashMenu(ref mut sm) = state.mode
                && let Some(cmd) = sm.selected_cmd()
            {
                state.input.set(format!("/{}", cmd.name));
                sm.update_matches(state.input.as_str());
            }
        }
        KeyCode::Enter => {
            let name = if let AppMode::SlashMenu(ref sm) = state.mode {
                sm.selected_cmd().map(|c| c.name.to_string())
            } else {
                None
            };
            if let Some(name) = name {
                state.input.clear();
                state.mode = AppMode::Chat;
                return SlashAction::ExecuteCommand(name);
            }
        }
        KeyCode::Backspace => {
            state.input.backspace();
            let should_exit = if let AppMode::SlashMenu(ref mut sm) = state.mode {
                sm.update_matches(state.input.as_str());
                !state.input.starts_with('/') || state.input == "/"
            } else {
                false
            };
            if should_exit {
                state.input.clear();
                state.mode = if state.is_streaming {
                    AppMode::Streaming
                } else {
                    AppMode::Chat
                };
            }
        }
        KeyCode::Char(c) if !key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) && !key.modifiers.contains(crossterm::event::KeyModifiers::ALT) => {
            state.input.insert(c);
            if let AppMode::SlashMenu(ref mut sm) = state.mode {
                sm.update_matches(state.input.as_str());
            }
        }
        _ => {}
    }
    SlashAction::None
}

pub fn draw(f: &mut Frame, state: &AppState, area: Rect, theme: &Theme) {
    let sm = match &state.mode {
        AppMode::SlashMenu(sm) => sm,
        _ => return,
    };
    let menu_width: u16 = 48;
    let menu_height = if sm.matched.is_empty() {
        3 // border + message + border
    } else {
        (sm.matched.len().min(8) as u16) + 2
    };
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

    if sm.matched.is_empty() {
        let msg = Line::from(Span::styled(
            " No matching commands",
            Style::default().fg(theme.muted),
        ));
        f.render_widget(Paragraph::new(msg), inner);
        return;
    }

    let name_col_width = 20usize;
    let inner_w = inner.width as usize;
    let items: Vec<Line> = sm
        .matched
        .iter()
        .enumerate()
        .map(|(vi, &ci)| {
            let cmd = &BUILTIN_COMMANDS[ci];
            let sel = vi == sm.selected;
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
