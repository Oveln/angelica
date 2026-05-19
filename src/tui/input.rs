use std::ops::Deref;
use unicode_width::UnicodeWidthChar;
use unicode_width::UnicodeWidthStr;

fn char_to_byte(char_idx: usize, s: &str) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(i, _)| i)
        .unwrap_or(s.len())
}

pub fn display_width_to_char_idx(width: usize, s: &str) -> usize {
    let mut w = 0;
    for (ci, c) in s.char_indices() {
        if w >= width {
            return ci;
        }
        w += c.width().unwrap_or(0);
    }
    s.len()
}

#[derive(Debug, Clone, PartialEq)]
pub struct InputBuffer {
    text: String,
    cursor: usize,
}

impl InputBuffer {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            cursor: 0,
        }
    }

    pub fn insert(&mut self, c: char) {
        let pos = char_to_byte(self.cursor, &self.text);
        self.text.insert(pos, c);
        self.cursor += 1;
    }

    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.cursor -= 1;
        let pos = char_to_byte(self.cursor, &self.text);
        let len = self.text[pos..]
            .chars()
            .next()
            .map(|c| c.len_utf8())
            .unwrap_or(0);
        self.text.drain(pos..pos + len);
    }

    pub fn delete(&mut self) {
        if self.cursor >= self.text.chars().count() {
            return;
        }
        let pos = char_to_byte(self.cursor, &self.text);
        let len = self.text[pos..]
            .chars()
            .next()
            .map(|c| c.len_utf8())
            .unwrap_or(0);
        self.text.drain(pos..pos + len);
    }

    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn move_right(&mut self) {
        if self.cursor < self.text.chars().count() {
            self.cursor += 1;
        }
    }

    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor = self.text.chars().count();
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
    }

    pub fn set(&mut self, text: String) {
        self.text = text;
        self.cursor = self.text.chars().count();
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    pub fn display_cursor_col(&self) -> u16 {
        let byte_pos = self
            .text
            .char_indices()
            .nth(self.cursor)
            .map(|(i, _)| i)
            .unwrap_or(self.text.len());
        UnicodeWidthStr::width(&self.text[..byte_pos]) as u16
    }
}

impl Deref for InputBuffer {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        &self.text
    }
}

impl Default for InputBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl PartialEq<&str> for InputBuffer {
    fn eq(&self, other: &&str) -> bool {
        self.text == *other
    }
}
