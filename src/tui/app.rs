use crossterm::{
    cursor::SetCursorStyle,
    event::{DisableMouseCapture, EnableMouseCapture, EventStream},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use futures::StreamExt;
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io::{self, Write};
use tokio::sync::mpsc;

use crate::agent::events::{AppEvent, UserAction};
use crate::tui::mode::{self, AppMode};
use crate::tui::state::AppState;

pub async fn run_tui(
    mut app_event_rx: mpsc::Receiver<AppEvent>,
    user_action_tx: mpsc::Sender<UserAction>,
    model_name: String,
    conversation_path: String,
) -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    execute!(stdout, EnableMouseCapture)?;
    stdout.write_all(b"\x1b[?1007h")?;
    stdout.flush()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut state = AppState::new(model_name).with_conversation_path(conversation_path);
    let mut reader = EventStream::new();
    let mut was_hovering_toggle = false;
    let drag_scroll_sleep = tokio::time::sleep(std::time::Duration::MAX);
    tokio::pin!(drag_scroll_sleep);

    loop {
        terminal.draw(|f| crate::tui::draw::draw(f, &mut state))?;

        if state.mouse.drag_scroll_pos.is_some() {
            drag_scroll_sleep
                .as_mut()
                .reset(tokio::time::Instant::now() + std::time::Duration::from_millis(80));
        }

        tokio::select! {
            event = app_event_rx.recv() => {
                match event {
                    Some(event) => {
                        state.handle_event(&event);
                        if matches!(event, AppEvent::Error { .. }) && state.messages.is_empty() {
                            state.should_quit = true;
                        }
                        if matches!(event, AppEvent::TurnComplete)
                            && let Some(msg) = state.queued_messages.pop_front()
                        {
                            state.add_chat("user", &msg, None);
                            let _ = user_action_tx.send(UserAction::SendMessage { content: msg }).await;
                        }
                    }
                    None => break,
                }
            }
            maybe_event = reader.next() => {
                match maybe_event {
                    Some(Ok(crossterm::event::Event::Key(key)))
                        if key.kind == crossterm::event::KeyEventKind::Press =>
                    {
                        handle_key(&mut state, key, &user_action_tx).await;
                    }
                    Some(Ok(crossterm::event::Event::Mouse(mouse))) => {
                        match mouse.kind {
                            crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                                state.handle_mouse_down(mouse.row, mouse.column);
                            }
                            crossterm::event::MouseEventKind::Drag(crossterm::event::MouseButton::Left) => {
                                state.handle_mouse_drag(mouse.row, mouse.column);
                            }
                            crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left) => {
                                if let Some(text) = state.handle_mouse_up() {
                                    let _ = copy_to_clipboard_osc52(&text);
                                    terminal.backend_mut().flush()?;
                                }
                            }
                            crossterm::event::MouseEventKind::Moved => {
                                let hovering = state.handle_hover(mouse.row, mouse.column);
                                if hovering != was_hovering_toggle {
                                    was_hovering_toggle = hovering;
                                    if hovering {
                                        execute!(terminal.backend_mut(), SetCursorStyle::SteadyUnderScore)?;
                                    } else {
                                        execute!(terminal.backend_mut(), SetCursorStyle::DefaultUserShape)?;
                                    }
                                    terminal.backend_mut().flush()?;
                                }
                            }
                            crossterm::event::MouseEventKind::ScrollUp => {
                                state.scroll.up(3);
                            }
                            crossterm::event::MouseEventKind::ScrollDown => {
                                state.scroll.down(3);
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
            _ = &mut drag_scroll_sleep, if state.mouse.drag_scroll_pos.is_some() => {
                let (row, col) = state.mouse.drag_scroll_pos.unwrap();
                state.handle_mouse_drag(row, col);
            }
        }

        if state.should_quit {
            break;
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), SetCursorStyle::DefaultUserShape)?;
    execute!(terminal.backend_mut(), DisableMouseCapture)?;
    terminal.backend_mut().write_all(b"\x1b[?1007l")?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

const BASE64_TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(BASE64_TABLE[((n >> 18) & 0x3F) as usize] as char);
        out.push(BASE64_TABLE[((n >> 12) & 0x3F) as usize] as char);
        out.push(if chunk.len() > 1 {
            BASE64_TABLE[((n >> 6) & 0x3F) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            BASE64_TABLE[(n & 0x3F) as usize] as char
        } else {
            '='
        });
    }
    out
}

fn copy_to_clipboard_osc52(text: &str) -> std::io::Result<()> {
    let encoded = base64_encode(text.as_bytes());
    let seq = format!("\x1b]52;c;{}\x07", encoded);
    std::io::stdout().write_all(seq.as_bytes())?;
    std::io::stdout().flush()
}

async fn handle_key(
    state: &mut AppState,
    key: crossterm::event::KeyEvent,
    tx: &mpsc::Sender<UserAction>,
) {
    if matches!(state.mode, AppMode::SlashMenu(_)) {
        let action = mode::slash::handle_key(state, key, tx).await;
        if let mode::slash::SlashAction::ExecuteCommand(name) = action {
            mode::execute_slash_command(state, &name, tx).await;
        }
        return;
    }

    if matches!(state.mode, AppMode::Welcome) {
        state.load_conversation();
        return;
    }

    match state.mode {
        AppMode::Approval(_) => {
            mode::approval::handle_key(state, key, tx).await;
        }
        _ => {
            mode::chat::handle_key(state, key, tx).await;
        }
    }
}
