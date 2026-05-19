use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use tokio::sync::mpsc;

use crate::session::SessionEntry;

use super::AppMode;
use crate::agent::events::UserAction;
use crate::tui::state::AppState;
use crate::tui::theme::Theme;

pub enum SessionAction {
    None,
    ResumeSession(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionPickerState {
    pub filter: String,
    pub selected: usize,
    pub matched: Vec<usize>,
    pub sessions: Vec<SessionEntry>,
}

impl SessionPickerState {
    pub fn new(sessions: Vec<SessionEntry>) -> Self {
        let matched = (0..sessions.len()).collect();
        Self {
            filter: String::new(),
            selected: 0,
            matched,
            sessions,
        }
    }

    pub fn rebuild_matches(&mut self) {
        self.matched.clear();
        let filter_lower = self.filter.to_lowercase();
        for (i, entry) in self.sessions.iter().enumerate() {
            if filter_lower.is_empty()
                || entry.session_id.to_lowercase().contains(&filter_lower)
                || entry.preview.to_lowercase().contains(&filter_lower)
            {
                self.matched.push(i);
            }
        }
        if self.selected >= self.matched.len() {
            self.selected = 0;
        }
    }

    pub fn selected_entry(&self) -> Option<&SessionEntry> {
        self.matched
            .get(self.selected)
            .and_then(|&i| self.sessions.get(i))
    }
}

pub async fn handle_key(
    state: &mut AppState,
    key: KeyEvent,
    _tx: &mpsc::Sender<UserAction>,
) -> SessionAction {
    match key.code {
        KeyCode::Esc => {
            state.mode = if state.is_streaming {
                AppMode::Streaming
            } else {
                AppMode::Chat
            };
        }
        KeyCode::Up => {
            if let AppMode::SessionPicker(ref mut sp) = state.mode {
                if sp.selected > 0 {
                    sp.selected -= 1;
                }
            }
        }
        KeyCode::Down => {
            if let AppMode::SessionPicker(ref mut sp) = state.mode {
                if sp.selected + 1 < sp.matched.len() {
                    sp.selected += 1;
                }
            }
        }
        KeyCode::Enter => {
            let session_id = if let AppMode::SessionPicker(ref sp) = state.mode {
                sp.selected_entry().map(|e| e.session_id.clone())
            } else {
                None
            };
            if let Some(id) = session_id {
                state.mode = AppMode::Chat;
                return SessionAction::ResumeSession(id);
            }
        }
        KeyCode::Backspace => {
            if let AppMode::SessionPicker(ref mut sp) = state.mode {
                sp.filter.pop();
                sp.rebuild_matches();
            }
        }
        KeyCode::Char(c) => {
            if let AppMode::SessionPicker(ref mut sp) = state.mode {
                sp.filter.push(c);
                sp.rebuild_matches();
            }
        }
        _ => {}
    }
    SessionAction::None
}

pub fn draw(f: &mut Frame, state: &AppState, area: Rect, theme: &Theme) {
    let sp = match &state.mode {
        AppMode::SessionPicker(sp) => sp,
        _ => return,
    };

    let dialog_width: u16 = 56;
    let max_rows: u16 = 12;
    let footer_height: u16 = 1;
    let border_h: u16 = 2;
    let list_rows = sp.matched.len().min(max_rows as usize) as u16;
    let dialog_height = border_h + list_rows + footer_height;

    let dialog = centered_rect(area, dialog_width, dialog_height);

    f.render_widget(Clear, dialog);

    let total = sp.matched.len();
    let title = if total > 0 {
        format!(" Resume ({}) ", total)
    } else {
        " Resume ".to_string()
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent))
        .title(title);
    f.render_widget(block, dialog);

    let inner = Rect {
        x: dialog.x + 1,
        y: dialog.y + 1,
        width: dialog.width.saturating_sub(2),
        height: dialog.height.saturating_sub(2),
    };

    // list area
    let list_area = Rect {
        x: inner.x,
        y: inner.y,
        width: inner.width,
        height: list_rows,
    };

    if sp.matched.is_empty() {
        let empty = Paragraph::new(Line::from(Span::styled(
            " No sessions found",
            Style::default().fg(theme.status_muted),
        )));
        f.render_widget(empty, list_area);
    } else {
        let total = sp.matched.len();
        let visible = total.min(max_rows as usize);
        let scroll_start = scroll_offset(sp.selected, total, visible);
        let scroll_end = scroll_start + visible;
        let inner_w = list_area.width as usize;

        let items: Vec<Line> = sp.matched[scroll_start..scroll_end]
            .iter()
            .enumerate()
            .map(|(vi, &ci)| {
                let entry = &sp.sessions[ci];
                let actual_idx = vi + scroll_start;
                let sel = actual_idx == sp.selected;
                let id_display = entry.session_id.clone();
                let max_preview = inner_w.saturating_sub(22);
                let preview: String = entry.preview.chars().take(max_preview).collect();

                if sel {
                    let fill = inner_w.saturating_sub(id_display.len() + preview.len() + 5);
                    Line::from(vec![
                        Span::styled(
                            format!(" \u{25B8} {} ", id_display),
                            Style::default()
                                .fg(ratatui::style::Color::Black)
                                .bg(theme.accent)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            preview,
                            Style::default()
                                .fg(ratatui::style::Color::Black)
                                .bg(theme.accent),
                        ),
                        Span::styled(" ".repeat(fill), Style::default().bg(theme.accent)),
                    ])
                } else {
                    Line::from(vec![
                        Span::styled(
                            format!("   {} ", id_display),
                            Style::default().fg(theme.tool),
                        ),
                        Span::styled(preview, Style::default().fg(theme.muted)),
                    ])
                }
            })
            .collect();

        f.render_widget(Paragraph::new(items), list_area);
    }

    // footer: hint
    let footer_y = inner.y + 1 + list_rows;
    let footer_area = Rect {
        x: inner.x,
        y: footer_y,
        width: inner.width,
        height: 1,
    };
    let total = sp.matched.len();
    let count_label = if total > 0 {
        format!("{} sessions", total)
    } else {
        String::new()
    };
    let hint = "Enter select \u{2502} Esc cancel \u{2502} \u{2191}\u{2193} navigate";
    let hint_line = Line::from(vec![
        Span::styled(
            format!(" {} ", hint),
            Style::default().fg(theme.status_muted),
        ),
        Span::styled(
            format!(" {}", count_label),
            Style::default().fg(theme.status_muted),
        ),
    ]);
    f.render_widget(Paragraph::new(hint_line), footer_area);
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    Rect {
        x: area.x + (area.width.saturating_sub(w)) / 2,
        y: area.y + (area.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    }
}

fn scroll_offset(selected: usize, total: usize, visible: usize) -> usize {
    if total <= visible {
        return 0;
    }
    let half = visible / 2;
    if selected < half {
        0
    } else if selected >= total - half {
        total - visible
    } else {
        selected - half
    }
}
