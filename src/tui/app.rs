use crossterm::{
    cursor::SetCursorStyle,
    event::{DisableMouseCapture, EnableMouseCapture, EventStream, KeyCode, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use futures::StreamExt;
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io::{self, Write};
use tokio::sync::mpsc;

use crate::agent::events::{AppEvent, UserAction};
use crate::tui::state::AppState;
use crate::tui::types::{AppMode, ApprovalChoice, BUILTIN_COMMANDS, DisplayMessage};

pub async fn run_tui(
    mut app_event_rx: mpsc::Receiver<AppEvent>,
    user_action_tx: mpsc::Sender<UserAction>,
    model_name: String,
) -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    execute!(stdout, EnableMouseCapture)?;
    stdout.write_all(b"\x1b[?1007h")?;
    stdout.flush()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut state = AppState::new(model_name);
    let mut reader = EventStream::new();
    let mut was_hovering_toggle = false;
    let drag_scroll_sleep = tokio::time::sleep(std::time::Duration::MAX);
    tokio::pin!(drag_scroll_sleep);

    loop {
        terminal.draw(|f| crate::tui::draw::draw(f, &mut state))?;

        if state.drag_scroll_pos.is_some() {
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
                        if matches!(event, AppEvent::TurnComplete) {
                            if let Some(msg) = state.queued_messages.pop_front() {
                                state.add_chat("user", &msg, None);
                                let _ = user_action_tx.send(UserAction::SendMessage { content: msg }).await;
                            }
                        }
                    }
                    None => break,
                }
            }
            maybe_event = reader.next() => {
                match maybe_event {
                    Some(Ok(crossterm::event::Event::Key(key))) => {
                        if key.kind == crossterm::event::KeyEventKind::Press {
                            handle_key(&mut state, key, &user_action_tx).await;
                        }
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
                                state.scroll_up(3);
                            }
                            crossterm::event::MouseEventKind::ScrollDown => {
                                state.scroll_down(3);
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
            _ = &mut drag_scroll_sleep, if state.drag_scroll_pos.is_some() => {
                let (row, col) = state.drag_scroll_pos.unwrap();
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
    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
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
    if state.mode == AppMode::SlashMenu {
        handle_slash_menu_key(state, key, tx).await;
        return;
    }

    match &state.mode {
        AppMode::Approval { tool_call_id, .. } => {
            let id = tool_call_id.clone();
            handle_approval_key(state, key, &id, tx).await;
        }
        _ => {
            handle_chat_key(state, key, tx).await;
        }
    }
}

async fn handle_chat_key(
    state: &mut AppState,
    key: crossterm::event::KeyEvent,
    tx: &mpsc::Sender<UserAction>,
) {
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

            if input.starts_with('/') {
                execute_slash_command(state, &input[1..], tx).await;
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
            state.mode = AppMode::SlashMenu;
            state.update_slash_matches();
        }
        KeyCode::Up => {
            if state.input.is_empty() && !state.queued_messages.is_empty() {
                if let Some(msg) = state.queued_messages.pop_back() {
                    state.input.set(msg);
                }
            } else {
                state.scroll_up(3);
            }
        }
        KeyCode::Char(c) => {
            state.input.insert(c);
            if state.input.starts_with('/') && state.input.chars().count() <= 15 {
                state.mode = AppMode::SlashMenu;
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
        KeyCode::Down => state.scroll_down(3),
        KeyCode::PageUp => state.scroll_up(10),
        KeyCode::PageDown => state.scroll_down(10),
        _ => {}
    }
}

async fn handle_slash_menu_key(
    state: &mut AppState,
    key: crossterm::event::KeyEvent,
    tx: &mpsc::Sender<UserAction>,
) {
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
            if state.slash_selected > 0 {
                state.slash_selected -= 1;
            }
        }
        KeyCode::Down => {
            if state.slash_selected + 1 < state.slash_matched.len() {
                state.slash_selected += 1;
            }
        }
        KeyCode::Tab => {
            if let Some(cmd) = state.slash_selected_cmd() {
                state.input.set(format!("/{}", cmd.name));
                state.update_slash_matches();
            }
        }
        KeyCode::Enter => {
            if let Some(cmd) = state.slash_selected_cmd() {
                let name = cmd.name.to_string();
                state.input.clear();
                state.mode = AppMode::Chat;
                execute_slash_command(state, &name, tx).await;
            }
        }
        KeyCode::Backspace => {
            state.input.backspace();
            state.update_slash_matches();
            if !state.input.starts_with('/') || state.input == "/" {
                state.input.clear();
                state.mode = if state.is_streaming {
                    AppMode::Streaming
                } else {
                    AppMode::Chat
                };
            }
        }
        KeyCode::Char(c) => {
            state.input.insert(c);
            state.update_slash_matches();
        }
        _ => {}
    }
}

async fn execute_slash_command(state: &mut AppState, cmd: &str, tx: &mpsc::Sender<UserAction>) {
    let (cmd_name, _arg) = match cmd.split_once(' ') {
        Some((n, a)) => (n, Some(a)),
        None => (cmd, None),
    };

    let cmd_lower = cmd_name.to_lowercase();

    let matched = BUILTIN_COMMANDS
        .iter()
        .find(|c| c.name == cmd_lower || c.aliases.iter().any(|a| *a == cmd_lower));

    if let Some(matched_cmd) = matched {
        match matched_cmd.name {
            "help" => {
                let mut help = String::from("Available commands:\n");
                for c in BUILTIN_COMMANDS {
                    let aliases = if c.aliases.is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", c.aliases.join(", "))
                    };
                    help.push_str(&format!(
                        "  /{}{}\n    {}\n",
                        c.name, aliases, c.description
                    ));
                }
                state.add_chat("system", &help, None);
            }
            "clear" => {
                state.messages.clear();
                let _ = tx.send(UserAction::ClearHistory).await;
            }
            "quit" | "q" => {
                state.should_quit = true;
                let _ = tx.send(UserAction::Quit).await;
            }
            "verbose" | "v" => {
                state.verbosity = state.verbosity.cycle();
                state.add_chat(
                    "system",
                    &format!("Verbosity: {}", state.verbosity.label()),
                    None,
                );
            }
            "thinking" | "think" => {
                state.thinking_visible = !state.thinking_visible;
                state.add_chat(
                    "system",
                    &format!(
                        "Thinking display: {}",
                        if state.thinking_visible { "on" } else { "off" }
                    ),
                    None,
                );
            }
            "model" => {
                let model = state.model_name.clone();
                state.add_chat("system", &model, None);
            }
            "history" | "h" => {
                let count = state.messages.len();
                let user_count = state
                    .messages
                    .iter()
                    .filter(|m| matches!(m, DisplayMessage::Chat { role, .. } if role == "user"))
                    .count();
                state.add_chat(
                    "system",
                    &format!("{} messages ({} user)", count, user_count),
                    None,
                );
            }
            _ => {
                state.add_chat("system", &format!("Unknown command: /{}", cmd_name), None);
            }
        }
    } else {
        state.add_chat(
            "system",
            &format!(
                "Unknown command: /{}. Type /help for available commands.",
                cmd_name
            ),
            None,
        );
    }
}

async fn handle_approval_key(
    state: &mut AppState,
    key: crossterm::event::KeyEvent,
    _tool_call_id: &str,
    tx: &mpsc::Sender<UserAction>,
) {
    let editing_feedback = state.approval_selected == ApprovalChoice::EditFeedback;

    match key.code {
        KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.should_quit = true;
            let _ = tx.send(UserAction::Quit).await;
        }
        KeyCode::Esc => {
            if editing_feedback {
                state.approval_selected = ApprovalChoice::Reject;
            }
        }

        KeyCode::Up => {
            state.scroll_up(3);
        }
        KeyCode::Down => {
            state.scroll_down(3);
        }
        KeyCode::PageUp => {
            state.scroll_up(10);
        }
        KeyCode::PageDown => {
            state.scroll_down(10);
        }
        KeyCode::Left => {
            if editing_feedback {
                state.feedback.move_left();
            } else {
                state.approval_selected = prev_choice(state.approval_selected);
            }
        }
        KeyCode::Right => {
            if editing_feedback {
                state.feedback.move_right();
            } else {
                state.approval_selected = next_choice(state.approval_selected);
            }
        }
        KeyCode::Tab => {
            state.approval_selected = next_choice(state.approval_selected);
        }

        KeyCode::Enter => match state.approval_selected {
            ApprovalChoice::Allow => {
                state.mode = AppMode::Chat;
                let _ = tx.send(UserAction::ApprovePending).await;
            }
            ApprovalChoice::Reject => {
                state.mode = AppMode::Chat;
                let _ = tx.send(UserAction::RejectTool { feedback: None }).await;
            }
            ApprovalChoice::EditFeedback => {
                if editing_feedback {
                    let feedback = state.feedback.trim().to_string();
                    state.feedback.clear();
                    state.mode = AppMode::Chat;
                    let _ = tx
                        .send(UserAction::RejectTool {
                            feedback: Some(feedback),
                        })
                        .await;
                } else {
                    state.approval_selected = ApprovalChoice::EditFeedback;
                }
            }
        },

        KeyCode::Char('y') if !editing_feedback => {
            state.mode = AppMode::Chat;
            let _ = tx.send(UserAction::ApprovePending).await;
        }
        KeyCode::Char('n') if !editing_feedback => {
            state.mode = AppMode::Chat;
            let _ = tx.send(UserAction::RejectTool { feedback: None }).await;
        }

        KeyCode::Char(c) if editing_feedback => {
            state.feedback.insert(c);
        }
        KeyCode::Backspace if editing_feedback => {
            state.feedback.backspace();
        }

        _ => {}
    }
}

fn next_choice(current: ApprovalChoice) -> ApprovalChoice {
    match current {
        ApprovalChoice::Allow => ApprovalChoice::Reject,
        ApprovalChoice::Reject => ApprovalChoice::EditFeedback,
        ApprovalChoice::EditFeedback => ApprovalChoice::Allow,
    }
}

fn prev_choice(current: ApprovalChoice) -> ApprovalChoice {
    match current {
        ApprovalChoice::Allow => ApprovalChoice::EditFeedback,
        ApprovalChoice::Reject => ApprovalChoice::Allow,
        ApprovalChoice::EditFeedback => ApprovalChoice::Reject,
    }
}
