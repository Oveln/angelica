use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc;

use super::{AppMode, SlashMenuState, execute_slash_command};
use crate::agent::events::UserAction;
use crate::tui::state::AppState;

pub async fn handle_key(state: &mut AppState, key: KeyEvent, tx: &mpsc::Sender<UserAction>) {
    match key.code {
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.should_quit = true;
            let _ = tx.send(UserAction::Quit).await;
        }
        KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.should_quit = true;
            let _ = tx.send(UserAction::Quit).await;
        }
        KeyCode::Esc => {
            if let Some(msg) = state.queued_messages.pop_back() {
                state.input.set(msg);
            }
        }
        KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.messages.clear();
            let _ = tx.send(UserAction::ClearHistory).await;
        }
        KeyCode::Char('o') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.toggle_last_collapsed();
        }
        KeyCode::Enter => {
            let input = state.input.trim().to_string();
            if input.is_empty() {
                return;
            }

            if let Some(stripped) = input.strip_prefix('/') {
                execute_slash_command(state, stripped, tx).await;
                state.input.clear();
                return;
            }

            state.input.clear();

            if state.is_streaming {
                state.queued_messages.push_back(input.clone());
                state.add_chat(
                    "system",
                    &format!("[queued: {}]", input.chars().take(40).collect::<String>()),
                    None,
                );
            } else {
                state.add_chat("user", &input, None);
                let _ = tx.send(UserAction::SendMessage { content: input }).await;
            }
        }
        KeyCode::Char('/') if state.input.is_empty() => {
            state.input.insert('/');
            state.mode = AppMode::SlashMenu(SlashMenuState::new());
            state.update_slash_matches();
        }
        KeyCode::Up => {
            if state.input.is_empty() && !state.queued_messages.is_empty() {
                if let Some(msg) = state.queued_messages.pop_back() {
                    state.input.set(msg);
                }
            } else {
                state.scroll.up(3);
            }
        }
        KeyCode::Char(c) => {
            state.input.insert(c);
            if state.input.starts_with('/') && state.input.chars().count() <= 15 {
                if !matches!(state.mode, AppMode::SlashMenu(_)) {
                    state.mode = AppMode::SlashMenu(SlashMenuState::new());
                }
                state.update_slash_matches();
            }
        }
        KeyCode::Backspace => {
            state.input.backspace();
            if state.input.starts_with('/') {
                state.update_slash_matches();
                if state.input == "/" {
                    state.mode = if state.is_streaming {
                        AppMode::Streaming
                    } else {
                        AppMode::Chat
                    };
                }
            }
        }
        KeyCode::Delete => state.input.delete(),
        KeyCode::Left => state.input.move_left(),
        KeyCode::Right => state.input.move_right(),
        KeyCode::Home => state.input.move_home(),
        KeyCode::End => state.input.move_end(),
        KeyCode::Down => state.scroll.down(3),
        KeyCode::PageUp => state.scroll.up(10),
        KeyCode::PageDown => state.scroll.down(10),
        _ => {}
    }
}
