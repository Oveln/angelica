use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::style::Modifier;

use super::AppMode;
use crate::input::InputBuffer;
use crate::theme::Theme;

/// A single row in the settings list.
#[derive(Debug, Clone)]
pub enum SettingsItem {
    Section {
        title: String,
    },
    Entry {
        path: String,
        key: String,
        value: String,
        original: String,
        editable: bool,
    },
}

#[derive(Debug, Clone)]
pub struct SettingsState {
    pub items: Vec<SettingsItem>,
    pub selected: usize,
    pub scroll: usize,
    pub editing: bool,
    pub edit_buffer: InputBuffer,
    pub dirty: bool,
    pub status_msg: Option<(String, bool)>,
    tree: toml::Value,
    original_tree: toml::Value,
    config_path: PathBuf,
}

impl SettingsState {
    pub async fn load() -> Option<Self> {
        let path = angelica::config::config_path();

        let path2 = path.clone();
        let raw = tokio::task::spawn_blocking(move || {
            if path2.exists() {
                std::fs::read_to_string(&path2).unwrap_or_default()
            } else {
                let default_config = angelica::config::Config::default();
                toml::to_string_pretty(&default_config).unwrap_or_default()
            }
        })
        .await
        .unwrap_or_default();

        let tree: toml::Value = toml::from_str(&raw).ok()?;
        let items = flatten_toml(&tree);
        let original_tree = tree.clone();

        Some(Self {
            items,
            selected: 0,
            scroll: 0,
            editing: false,
            edit_buffer: InputBuffer::new(),
            dirty: false,
            status_msg: None,
            tree,
            original_tree,
            config_path: path,
        })
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if self.editing {
            self.handle_edit_key(key);
            return;
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                // Caller handles closing
            }
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::PageUp => self.move_selection(-10),
            KeyCode::PageDown => self.move_selection(10),
            KeyCode::Enter => self.start_edit(),
            // 's' (save) handled in app.rs via save()
            KeyCode::Char('r') => self.reset_entry(),
            KeyCode::Char('d') => self.reset_all(),
            _ => {}
        }
    }
    fn move_selection(&mut self, delta: i32) {
        let new = self.selected as i32 + delta;
        let max = self.items.len().saturating_sub(1) as i32;
        self.selected = new.clamp(0, max) as usize;
    }

    fn start_edit(&mut self) {
        let idx = self.selected;
        if let Some(SettingsItem::Entry {
            value, editable, ..
        }) = self.items.get(idx)
        {
            if !editable {
                return;
            }
            self.editing = true;
            self.edit_buffer.set(value.clone());
        }
    }

    fn handle_edit_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => self.confirm_edit(),
            KeyCode::Esc => self.cancel_edit(),
            KeyCode::Char(c) if !key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) && !key.modifiers.contains(crossterm::event::KeyModifiers::ALT) => self.edit_buffer.insert(c),
            KeyCode::Backspace => self.edit_buffer.backspace(),
            KeyCode::Delete => self.edit_buffer.delete(),
            KeyCode::Left => self.edit_buffer.move_left(),
            KeyCode::Right => self.edit_buffer.move_right(),
            KeyCode::Home => self.edit_buffer.move_home(),
            KeyCode::End => self.edit_buffer.move_end(),
            _ => {}
        }
    }

    fn confirm_edit(&mut self) {
        let idx = self.selected;
        let new_value = self.edit_buffer.as_str().to_string();

        let (path, changed) = {
            let Some(SettingsItem::Entry { path, value, .. }) = self.items.get_mut(idx) else {
                self.editing = false;
                self.edit_buffer.clear();
                return;
            };
            if &new_value != value {
                *value = new_value;
                self.dirty = true;
                (path.clone(), true)
            } else {
                (String::new(), false)
            }
        };

        if changed {
            set_toml_value(&mut self.tree, &path, &self.items[idx]);
        }

        self.editing = false;
        self.edit_buffer.clear();
    }

    fn cancel_edit(&mut self) {
        self.editing = false;
        self.edit_buffer.clear();
    }

    fn reset_entry(&mut self) {
        let idx = self.selected;
        if let Some(SettingsItem::Entry {
            value, original, ..
        }) = self.items.get_mut(idx)
        {
            if value != original {
                *value = original.clone();
                set_toml_value(
                    &mut self.tree,
                    &get_path(&self.items[idx]),
                    &self.items[idx],
                );
                self.dirty = true;
            }
        }
    }

    fn reset_all(&mut self) {
        self.tree = self.original_tree.clone();
        self.items = flatten_toml(&self.tree);
        self.dirty = false;
        self.status_msg = Some(("Reset to original values".to_string(), false));
    }

    pub async fn save(&mut self) {
        let toml_str = match toml::to_string_pretty(&self.tree) {
            Ok(s) => s,
            Err(e) => {
                self.status_msg = Some((format!("Serialize error: {}", e), true));
                return;
            }
        };

        match angelica::config::Config::parse_toml(&toml_str) {
            Ok(_) => {}
            Err(e) => {
                self.status_msg = Some((format!("Validation error: {}", e), true));
                return;
            }
        }

        let path = self.config_path.clone();
        let write_result = tokio::task::spawn_blocking(move || -> std::io::Result<()> {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let tmp_path = path.with_extension("toml.tmp");
            std::fs::write(&tmp_path, &toml_str)?;
            if let Err(e) = std::fs::rename(&tmp_path, &path) {
                let _ = std::fs::remove_file(&tmp_path);
                return Err(e);
            }
            Ok(())
        })
        .await
        .unwrap_or(Err(std::io::Error::other("spawn_blocking failed")));

        match write_result {
            Ok(()) => {}
            Err(e) => {
                self.status_msg = Some((format!("Write error: {}", e), true));
                return;
            }
        }

        self.original_tree = self.tree.clone();
        let original_items = flatten_toml(&self.original_tree);
        for (i, item) in self.items.iter_mut().enumerate() {
            if let (SettingsItem::Entry { original, .. }, SettingsItem::Entry { value, .. }) =
                (item, &original_items[i])
            {
                *original = value.clone();
            }
        }

        self.dirty = false;
        self.status_msg = Some(("Saved to config.toml (restart to apply)".to_string(), false));
    }
}

// -- TOML tree helpers --

fn flatten_toml(table: &toml::Value) -> Vec<SettingsItem> {
    let mut items = Vec::new();
    let Some(table) = table.as_table() else {
        return items;
    };

    // Ordered section list
    let section_order = [
        "llm",
        "memory",
        "embedding",
        "fatigue",
        "mcp",
        "state",
        "skills",
        "permission",
    ];

    for section_name in &section_order {
        let Some(section_val) = table.get(*section_name) else {
            continue;
        };
        let Some(_section_table) = section_val.as_table() else {
            continue;
        };

        items.push(SettingsItem::Section {
            title: section_name.to_string(),
        });

        flatten_section(
            &mut items,
            section_name,
            section_name,
            section_val,
            section_val,
        );
    }

    items
}

fn flatten_section(
    items: &mut Vec<SettingsItem>,
    section: &str,
    prefix: &str,
    current: &toml::Value,
    original: &toml::Value,
) {
    let Some(table) = current.as_table() else {
        return;
    };

    for (key, value) in table {
        let path = format!("{}.{}", prefix, key);

        match value {
            toml::Value::Table(_) => {
                // Nested table: recurse with display key
                flatten_section(items, section, &path, value, original);
            }
            toml::Value::Array(arr) => {
                // Array of tables (e.g., providers): flatten with index
                for (i, item) in arr.iter().enumerate() {
                    if item.is_table() {
                        // Add a sub-header
                        items.push(SettingsItem::Section {
                            title: format!("{}.{}[{}]", section, key, i),
                        });
                        flatten_section(
                            items,
                            section,
                            &format!("{}.{}[{}]", prefix, key, i),
                            item,
                            original,
                        );
                    }
                }
            }
            _ => {
                let original_value = get_original_value(original, &path, prefix, section)
                    .unwrap_or_else(|| format_toml_value(value));
                let editable = is_editable(value);
                items.push(SettingsItem::Entry {
                    path: path.clone(),
                    key: format_key(&path, section),
                    value: format_toml_value(value),
                    original: original_value,
                    editable,
                });
            }
        }
    }
}

fn get_original_value(
    original_root: &toml::Value,
    full_path: &str,
    _prefix: &str,
    _section: &str,
) -> Option<String> {
    // Try to get the original value at the same path
    let parts: Vec<&str> = full_path.split('.').collect();
    let mut current = original_root;
    for part in &parts[1..] {
        // Handle array index: "providers[0]"
        if let Some(bracket) = part.find('[') {
            let key = &part[..bracket];
            let idx_str = &part[bracket + 1..part.len() - 1];
            let idx: usize = idx_str.parse().ok()?;
            current = current.as_table()?.get(key)?.as_array()?.get(idx)?;
        } else {
            current = current.as_table()?.get(*part)?;
        }
    }
    Some(format_toml_value(current))
}

fn format_toml_value(value: &toml::Value) -> String {
    match value {
        toml::Value::String(s) => s.clone(),
        toml::Value::Integer(n) => n.to_string(),
        toml::Value::Float(f) => format!("{:.6}", f)
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string(),
        toml::Value::Boolean(b) => b.to_string(),
        toml::Value::Datetime(d) => d.to_string(),
        _ => String::new(),
    }
}

fn is_editable(value: &toml::Value) -> bool {
    matches!(
        value,
        toml::Value::String(_)
            | toml::Value::Integer(_)
            | toml::Value::Float(_)
            | toml::Value::Boolean(_)
    )
}

fn format_key(path: &str, section: &str) -> String {
    // "llm.max_iterations" -> "max_iterations"
    // "llm.providers[0].model" -> "[0].model"
    let stripped = path
        .strip_prefix(section)
        .unwrap_or(path)
        .trim_start_matches('.');
    stripped.to_string()
}

fn get_path(item: &SettingsItem) -> String {
    match item {
        SettingsItem::Entry { path, .. } => path.clone(),
        _ => String::new(),
    }
}

fn set_toml_value(tree: &mut toml::Value, path: &str, item: &SettingsItem) {
    let SettingsItem::Entry { value, .. } = item else {
        return;
    };

    // Parse path: "llm.providers[0].model"
    let parts: Vec<&str> = path.split('.').collect();
    if parts.is_empty() {
        return;
    }

    // Walk to the parent, then set the leaf key
    let mut current = tree;
    for part in &parts[..parts.len() - 1] {
        if let Some(bracket) = part.find('[') {
            let key = &part[..bracket];
            let idx_str = &part[bracket + 1..part.len() - 1];
            let idx: usize = match idx_str.parse() {
                Ok(i) => i,
                Err(_) => return,
            };
            let arr = match current.as_table_mut().and_then(|t| t.get_mut(key)) {
                Some(toml::Value::Array(a)) => a,
                _ => return,
            };
            current = match arr.get_mut(idx) {
                Some(v) => v,
                None => return,
            };
        } else {
            current = match current.as_table_mut().and_then(|t| t.get_mut(*part)) {
                Some(v) => v,
                None => return,
            };
        }
    }

    // Set the leaf value
    let leaf_key = parts.last().unwrap();
    let new_val = parse_value_string(value);
    if let Some(table) = current.as_table_mut() {
        table.insert(leaf_key.to_string(), new_val);
    }
}

fn parse_value_string(s: &str) -> toml::Value {
    if s == "true" {
        return toml::Value::Boolean(true);
    }
    if s == "false" {
        return toml::Value::Boolean(false);
    }
    if let Ok(n) = s.parse::<i64>() {
        return toml::Value::Integer(n);
    }
    if let Ok(f) = s.parse::<f64>() {
        return toml::Value::Float(f);
    }
    toml::Value::String(s.to_string())
}

// -- Rendering --

pub fn draw(
    f: &mut ratatui::Frame,
    state: &mut crate::state::AppState,
    area: ratatui::layout::Rect,
    theme: &Theme,
) {
    let AppMode::Settings(ref settings) = state.mode else {
        return;
    };

    let popup_area = centered_rect(area, 80, 85);
    f.render_widget(ratatui::widgets::Clear, popup_area);

    let dirty_marker = if settings.dirty { " \u{26A1}" } else { "" };
    let block = ratatui::widgets::Block::default()
        .borders(ratatui::widgets::Borders::ALL)
        .border_style(ratatui::style::Style::default().fg(theme.accent))
        .title(ratatui::text::Span::styled(
            format!(" Settings{} ", dirty_marker),
            ratatui::style::Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let items = &settings.items;
    if items.is_empty() {
        let empty = ratatui::widgets::Paragraph::new(ratatui::text::Line::from(
            ratatui::text::Span::styled(
                "No settings found.",
                ratatui::style::Style::default().fg(theme.muted),
            ),
        ));
        f.render_widget(empty, inner);
        return;
    }

    let visible_height = inner.height as usize;
    let selected = settings.selected;

    // Compute scroll to keep selected visible
    let scroll = if selected < settings.scroll {
        selected
    } else if selected >= settings.scroll + visible_height {
        selected.saturating_sub(visible_height - 1)
    } else {
        settings.scroll
    };
    let computed_scroll = scroll.min(items.len().saturating_sub(1));

    let mut lines: Vec<ratatui::text::Line> = Vec::new();
    let mut editing_row: Option<u16> = None;
    let mut editing_col: u16 = 0;

    for (i, item) in items.iter().enumerate() {
        let is_selected = i == selected;

        match item {
            SettingsItem::Section { title } => {
                let marker = if is_selected { "\u{25B6} " } else { "  " };
                lines.push(ratatui::text::Line::from(ratatui::text::Span::styled(
                    format!("{}\u{25AA} {}", marker, title),
                    ratatui::style::Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                )));
            }
            SettingsItem::Entry {
                key,
                value,
                original,
                editable,
                ..
            } => {
                let modified = value != original;
                let marker = if is_selected { "\u{25B8}" } else { " " };
                let key_width = 28;
                let padded_key = format!("{} {:<width$}", marker, key, width = key_width);

                let key_style = if is_selected {
                    ratatui::style::Style::default()
                        .fg(theme.status_fg)
                        .add_modifier(Modifier::BOLD)
                } else {
                    ratatui::style::Style::default().fg(theme.status_muted)
                };

                let sep = ratatui::text::Span::styled(
                    " = ",
                    ratatui::style::Style::default().fg(theme.muted),
                );

                let val_style = if !editable {
                    ratatui::style::Style::default().fg(theme.muted)
                } else if modified {
                    ratatui::style::Style::default().fg(theme.warning)
                } else {
                    ratatui::style::Style::default().fg(theme.status_fg)
                };

                let display_val = if settings.editing && is_selected {
                    settings.edit_buffer.as_str().to_string()
                } else {
                    value.clone()
                };

                lines.push(ratatui::text::Line::from(vec![
                    ratatui::text::Span::styled(padded_key, key_style),
                    sep,
                    ratatui::text::Span::styled(display_val, val_style),
                ]));

                if settings.editing && is_selected {
                    let row_in_view = i.saturating_sub(computed_scroll);
                    if row_in_view < visible_height {
                        editing_row = Some(row_in_view as u16);
                        editing_col = (3
                            + key_width
                            + 3
                            + settings.edit_buffer.display_cursor_col() as usize)
                            as u16;
                    }
                }
            }
        }
    }

    // Help bar
    let help = if settings.editing {
        " Enter: confirm  Esc: cancel".to_string()
    } else {
        let dirty = if settings.dirty { " *unsaved*" } else { "" };
        format!(
            "\u{2191}\u{2193}/j/k: nav  Enter: edit  s: save  r: reset  d: reset all  q/Esc: close{}  ",
            dirty
        )
    };
    lines.push(ratatui::text::Line::from(""));
    lines.push(ratatui::text::Line::from(ratatui::text::Span::styled(
        help,
        ratatui::style::Style::default().fg(theme.status_muted),
    )));

    // Status message
    if let Some((msg, is_error)) = &settings.status_msg {
        let color = if *is_error {
            theme.error
        } else {
            theme.success
        };
        lines.push(ratatui::text::Line::from(ratatui::text::Span::styled(
            format!(" {}", msg),
            ratatui::style::Style::default().fg(color),
        )));
    }

    // Render visible slice
    let visible: Vec<ratatui::text::Line> = lines
        .iter()
        .skip(computed_scroll)
        .take(visible_height)
        .cloned()
        .collect();

    let para = ratatui::widgets::Paragraph::new(visible);
    f.render_widget(para, inner);

    // Cursor for editing
    if let Some(row) = editing_row {
        f.set_cursor_position((inner.x + editing_col, inner.y + row));
    }
}

fn centered_rect(
    area: ratatui::layout::Rect,
    percent_x: u16,
    percent_y: u16,
) -> ratatui::layout::Rect {
    let popup_width = area.width * percent_x / 100;
    let popup_height = area.height * percent_y / 100;
    ratatui::layout::Rect {
        x: area.x + (area.width.saturating_sub(popup_width)) / 2,
        y: area.y + (area.height.saturating_sub(popup_height)) / 2,
        width: popup_width,
        height: popup_height,
    }
}
