use crossterm::{
    event::{EventStream, KeyCode, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use futures::StreamExt;
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io::{self, Write};
use tokio::sync::mpsc;

use crate::agent::events::{AppEvent, UserAction};
use crate::tui::ui::{AppMode, AppState, ApprovalChoice, BUILTIN_COMMANDS};

pub async fn run_tui(
    mut app_event_rx: mpsc::Receiver<AppEvent>,
    user_action_tx: mpsc::Sender<UserAction>,
) -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    stdout.write_all(b"\x1b[?1007h")?;
    stdout.flush()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut state = AppState::default();
    let mut reader = EventStream::new();

    loop {
        terminal.draw(|f| crate::tui::ui::draw(f, &mut state))?;

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
                                state.add_message("user", &msg, None);
                                let _ = user_action_tx.send(UserAction::SendMessage { content: msg }).await;
                            }
                        }
                    }
                    None => break,
                }
            }
            maybe_event = reader.next() => {
                if let Some(Ok(crossterm::event::Event::Key(key))) = maybe_event {
                    if key.kind == crossterm::event::KeyEventKind::Press {
                        handle_key(&mut state, key, &user_action_tx, &mut terminal).await;
                    }
                }
            }
        }

        if state.should_quit {
            break;
        }
    }

    disable_raw_mode()?;
    terminal.backend_mut().write_all(b"\x1b[?1007l")?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

async fn handle_key(
    state: &mut AppState,
    key: crossterm::event::KeyEvent,
    tx: &mpsc::Sender<UserAction>,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) {
    if state.mode == AppMode::SlashMenu {
        handle_slash_menu_key(state, key, tx).await;
        return;
    }

    let mode_clone = state.mode.clone();
    match mode_clone {
        AppMode::Approval {
            is_tty_command,
            command,
            ..
        } => {
            handle_approval_key(state, key, is_tty_command, command.as_deref(), tx, terminal).await;
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
            if state.is_streaming {
                state.is_streaming = false;
                state.thinking_buffer.clear();
                state.text_buffer.clear();
                state.mode = AppMode::Chat;
                state.add_message("system", "[interrupted]", None);
                let _ = tx.send(UserAction::Interrupt).await;
            } else if let Some(msg) = state.queued_messages.pop_back() {
                state.input = msg;
                state.input_cursor_char = state.input.chars().count();
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
                state.input_clear();
                return;
            }

            state.input_clear();

            if state.is_streaming {
                state.queued_messages.push_back(input.clone());
                state.add_message("system", &format!("[queued: {}]", input.chars().take(40).collect::<String>()), None);
            } else {
                state.add_message("user", &input, None);
                let _ = tx.send(UserAction::SendMessage { content: input }).await;
            }
        }
        KeyCode::Char('/') if state.input.is_empty() => {
            state.input_insert('/');
            state.mode = AppMode::SlashMenu;
            state.update_slash_matches();
        }
        KeyCode::Up => {
            if state.input.is_empty() && !state.queued_messages.is_empty() {
                if let Some(msg) = state.queued_messages.pop_back() {
                    state.input = msg;
                    state.input_cursor_char = state.input.chars().count();
                }
            } else {
                state.scroll_up(3);
            }
        }
        KeyCode::Char(c) => {
            state.input_insert(c);
            if state.input.starts_with('/') && state.input.chars().count() <= 15 {
                state.mode = AppMode::SlashMenu;
                state.update_slash_matches();
            }
        }
        KeyCode::Backspace => {
            state.input_backspace();
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
        KeyCode::Delete => state.input_delete(),
        KeyCode::Left => state.input_move_left(),
        KeyCode::Right => state.input_move_right(),
        KeyCode::Home => state.input_move_home(),
        KeyCode::End => state.input_move_end(),
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
            state.input_clear();
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
                state.input = format!("/{}", cmd.name);
                state.input_cursor_char = state.input.chars().count();
                state.update_slash_matches();
            }
        }
        KeyCode::Enter => {
            if let Some(cmd) = state.slash_selected_cmd() {
                let name = cmd.name.to_string();
                state.input_clear();
                state.mode = AppMode::Chat;
                execute_slash_command(state, &name, tx).await;
            }
        }
        KeyCode::Backspace => {
            state.input_backspace();
            state.update_slash_matches();
            if !state.input.starts_with('/') || state.input == "/" {
                state.input_clear();
                state.mode = if state.is_streaming {
                    AppMode::Streaming
                } else {
                    AppMode::Chat
                };
            }
        }
        KeyCode::Char(c) => {
            state.input_insert(c);
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

    let matched = BUILTIN_COMMANDS.iter().find(|c| {
        c.name == cmd_lower || c.aliases.iter().any(|a| *a == cmd_lower)
    });

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
                    help.push_str(&format!("  /{}{}\n    {}\n", c.name, aliases, c.description));
                }
                state.add_message("system", &help, None);
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
                state.add_message(
                    "system",
                    &format!("Verbosity: {}", state.verbosity.label()),
                    None,
                );
            }
            "thinking" | "think" => {
                state.thinking_visible = !state.thinking_visible;
                state.add_message(
                    "system",
                    &format!(
                        "Thinking display: {}",
                        if state.thinking_visible { "on" } else { "off" }
                    ),
                    None,
                );
            }
            "model" => {
                state.add_message("system", "deepseek-v4-flash", None);
            }
            "history" | "h" => {
                let count = state.messages.len();
                let user_count = state.messages.iter().filter(|m| m.role == "user").count();
                state.add_message(
                    "system",
                    &format!("{} messages ({} user)", count, user_count),
                    None,
                );
            }
            _ => {
                state.add_message(
                    "system",
                    &format!("Unknown command: /{}", cmd_name),
                    None,
                );
            }
        }
    } else {
        state.add_message(
            "system",
            &format!("Unknown command: /{}. Type /help for available commands.", cmd_name),
            None,
        );
    }
}

async fn handle_approval_key(
    state: &mut AppState,
    key: crossterm::event::KeyEvent,
    is_tty_command: bool,
    command: Option<&str>,
    tx: &mpsc::Sender<UserAction>,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) {
    let editing_feedback = state.approval_selected == ApprovalChoice::EditFeedback;

    match key.code {
        KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.should_quit = true;
            let _ = tx.send(UserAction::Quit).await;
        }
        KeyCode::Esc => {
            state.mode = AppMode::Chat;
            let _ = tx.send(UserAction::RejectCommand { feedback: None }).await;
        }

        KeyCode::Up => {
            if editing_feedback {
                state.approval_selected = ApprovalChoice::Reject;
            } else {
                state.scroll_up(3);
            }
        }
        KeyCode::Down => {
            if editing_feedback {
                // nop
            } else {
                state.scroll_down(3);
            }
        }
        KeyCode::PageUp => {
            state.scroll_up(10);
        }
        KeyCode::PageDown => {
            state.scroll_down(10);
        }
        KeyCode::Left => {
            if editing_feedback {
                state.feedback_move_left();
            } else {
                state.approval_selected = prev_choice(state.approval_selected);
            }
        }
        KeyCode::Right => {
            if editing_feedback {
                state.feedback_move_right();
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
                if is_tty_command {
                    if let Some(cmd) = command {
                        let output = run_interactive(terminal, cmd);
                        state.handle_event(&AppEvent::CommandResult {
                            output: output.clone(),
                        });
                        let _ = tx.send(UserAction::ApprovePendingWithResult { output }).await;
                    }
                } else {
                    let _ = tx.send(UserAction::ApprovePending).await;
                }
            }
            ApprovalChoice::Reject => {
                state.mode = AppMode::Chat;
                let _ = tx.send(UserAction::RejectCommand { feedback: None }).await;
            }
            ApprovalChoice::EditFeedback => {
                if editing_feedback {
                    let feedback = state.approval_feedback.trim().to_string();
                    state.feedback_clear();
                    state.mode = AppMode::Chat;
                    let _ = tx
                        .send(UserAction::RejectCommand {
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
            if is_tty_command {
                if let Some(cmd) = command {
                    let output = run_interactive(terminal, cmd);
                    state.handle_event(&AppEvent::CommandResult {
                        output: output.clone(),
                    });
                    let _ = tx.send(UserAction::ApprovePendingWithResult { output }).await;
                }
            } else {
                let _ = tx.send(UserAction::ApprovePending).await;
            }
        }
        KeyCode::Char('n') if !editing_feedback => {
            state.mode = AppMode::Chat;
            let _ = tx.send(UserAction::RejectCommand { feedback: None }).await;
        }

        KeyCode::Char(c) if editing_feedback => {
            state.feedback_insert(c);
        }
        KeyCode::Backspace if editing_feedback => {
            state.feedback_backspace();
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

fn run_interactive(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, command: &str) -> String {
    disable_raw_mode().ok();
    execute!(io::stdout(), LeaveAlternateScreen).ok();

    println!("$ {}", command);

    let result = std::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .status();

    execute!(io::stdout(), EnterAlternateScreen).ok();
    io::stdout().write_all(b"\x1b[?1007h").ok();
    io::stdout().flush().ok();
    enable_raw_mode().ok();
    terminal.clear().ok();

    match result {
        Ok(status) => {
            let code = status.code().unwrap_or(-1);
            if status.success() {
                format!("[interactive command completed, exit code: {}]", code)
            } else {
                format!("[interactive command failed, exit code: {}]", code)
            }
        }
        Err(e) => format!("[error: {}]", e),
    }
}
