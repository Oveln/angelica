use ratatui::style::Color;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthChar;
use unicode_width::UnicodeWidthStr;

pub(super) fn wrap_str(s: &str, max_w: usize) -> Vec<String> {
    if max_w == 0 {
        return vec![s.to_string()];
    }
    let mut result = Vec::new();
    let mut w = 0;
    let mut last = 0;
    for (i, c) in s.char_indices() {
        let cw = c.width().unwrap_or(0);
        if w + cw > max_w {
            result.push(s[last..i].to_string());
            last = i;
            w = cw;
        } else {
            w += cw;
        }
    }
    if last < s.len() {
        result.push(s[last..].to_string());
    }
    if result.is_empty() {
        result.push(String::new());
    }
    result
}

pub(super) fn split_at_display_width(s: &str, width: usize) -> (&str, &str) {
    let mut w = 0;
    for (i, c) in s.char_indices() {
        if w >= width {
            return (&s[..i], &s[i..]);
        }
        w += c.width().unwrap_or(0);
    }
    (s, "")
}

pub(super) fn apply_line_selection(line: &mut Line, sel_start: usize, sel_end: usize, bg: Color) {
    let mut new_spans = Vec::new();
    let mut col = 0;

    for span in line.spans.drain(..) {
        let span_width = UnicodeWidthStr::width(span.content.as_ref());
        let span_end = col + span_width;

        if span_end <= sel_start || col >= sel_end {
            new_spans.push(span);
            col = span_end;
            continue;
        }

        let text: &str = span.content.as_ref();
        let style = span.style;

        let before_w = sel_start.saturating_sub(col);
        let selected_w = sel_end.min(span_end).saturating_sub(sel_start.max(col));

        let (before, rest) = split_at_display_width(text, before_w);
        let (selected, after) = split_at_display_width(rest, selected_w);

        if !before.is_empty() {
            new_spans.push(Span::styled(before.to_string(), style));
        }
        if !selected.is_empty() {
            new_spans.push(Span::styled(selected.to_string(), style.bg(bg)));
        }
        if !after.is_empty() {
            new_spans.push(Span::styled(after.to_string(), style));
        }

        col = span_end;
    }

    line.spans = new_spans;
}
