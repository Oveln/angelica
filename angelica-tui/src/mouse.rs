use super::input::display_width_to_char_idx;
use super::state::AppState;
use super::types::DisplayMessage;

impl AppState {
    pub fn handle_mouse_down(&mut self, row: u16, col: u16) {
        let abs = self.screen_to_content(row);
        let (abs, col) = match abs {
            Some(v) => (v, col as usize),
            None => return,
        };
        self.mouse.mouse_down_pos = Some((abs, col));
        self.mouse.mouse_down_on_toggle = None;

        for range in self.viewport.clickable_ranges.iter() {
            if abs == range.line && col >= range.col_start && col < range.col_end {
                self.mouse.mouse_down_on_toggle = Some((abs, col));
                return;
            }
        }
    }

    pub fn handle_mouse_drag(&mut self, row: u16, col: u16) {
        if self.mouse.mouse_down_on_toggle.is_some() {
            self.mouse.mouse_down_on_toggle = None;
        }

        let Some((start_line, start_col)) = self.mouse.mouse_down_pos else {
            return;
        };

        let area = self.viewport.messages_area;
        let col_usize = col as usize;
        let at_edge = row <= area.y || row >= area.y + area.height;

        if self.mouse.selection.is_some() && at_edge {
            if row <= area.y {
                self.scroll.up(1);
            } else {
                self.scroll.down(1);
            }
            self.mouse.drag_scroll_pos = Some((row, col));
        } else if !at_edge {
            self.mouse.drag_scroll_pos = None;
        }

        let clamped_row = row.clamp(area.y, area.y + area.height.saturating_sub(1));
        let abs = match self.screen_to_content(clamped_row) {
            Some(v) => v,
            None => return,
        };

        if self.mouse.selection.is_none()
            && (abs != start_line || col_usize.abs_diff(start_col) > 2)
        {
            self.mouse.selection = Some((start_line, start_col, abs, col_usize));
        } else if self.mouse.selection.is_some() {
            let (s_line, s_col, _, _) = self.mouse.selection.unwrap();
            self.mouse.selection = Some((s_line, s_col, abs, col_usize));
        }
    }

    pub fn handle_mouse_up(&mut self) -> Option<String> {
        if let Some((_line, _col)) = self.mouse.mouse_down_on_toggle.take() {
            for range in self.viewport.clickable_ranges.iter() {
                if _line == range.line && _col >= range.col_start && _col < range.col_end {
                    self.toggle_by_index(range.msg_index);
                    self.viewport.hovered_msg_index = None;
                    break;
                }
            }
            return None;
        }

        let copied = if let Some(sel) = self.mouse.selection.take() {
            let (sl, sc, el, ec) = sel;
            if sl != el || sc != ec {
                Some(self.extract_selected_text(sl, sc, el, ec))
            } else {
                None
            }
        } else {
            None
        };

        self.mouse.mouse_down_pos = None;
        self.mouse.mouse_down_on_toggle = None;
        self.mouse.drag_scroll_pos = None;
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
            if line_idx >= self.viewport.cached_line_texts.len() {
                break;
            }
            let text = &self.viewport.cached_line_texts[line_idx];
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
                if self.viewport.hovered_msg_index.is_some() {
                    self.viewport.hovered_msg_index = None;
                }
                return false;
            }
        };
        let col = col as usize;
        for range in self.viewport.clickable_ranges.iter() {
            if abs == range.line && col >= range.col_start && col < range.col_end {
                if self.viewport.hovered_msg_index != Some(range.msg_index) {
                    self.viewport.hovered_msg_index = Some(range.msg_index);
                }
                return true;
            }
        }
        if self.viewport.hovered_msg_index.is_some() {
            self.viewport.hovered_msg_index = None;
        }
        false
    }

    fn screen_to_content(&self, row: u16) -> Option<usize> {
        let area = self.viewport.messages_area;
        if row < area.y || row >= area.y + area.height {
            return None;
        }
        let visible_row = (row - area.y) as usize;
        let visible_height = area.height as usize;
        let padding = visible_height.saturating_sub(self.viewport.content_height);
        if visible_row < padding {
            return None;
        }
        Some(self.viewport.content_top + visible_row - padding)
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
}
