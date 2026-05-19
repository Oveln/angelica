use super::state::AppState;

const TAIL_SENTINEL: usize = usize::MAX;

impl AppState {
    pub fn is_at_tail(&self) -> bool {
        self.scroll_offset == TAIL_SENTINEL
    }

    pub fn resolve_top(&self, max_start: usize) -> usize {
        if self.scroll_offset == TAIL_SENTINEL {
            max_start
        } else {
            self.scroll_offset.min(max_start)
        }
    }

    pub fn scroll_up(&mut self, n: usize) {
        self.pending_scroll_delta -= n as i32;
    }

    pub fn scroll_down(&mut self, n: usize) {
        self.pending_scroll_delta += n as i32;
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = TAIL_SENTINEL;
        self.pending_scroll_delta = 0;
    }

    pub fn apply_pending_scroll(&mut self, total_lines: usize, visible_lines: usize) {
        let delta = self.pending_scroll_delta;
        if delta == 0 {
            return;
        }
        self.pending_scroll_delta = 0;

        if total_lines <= visible_lines {
            self.scroll_offset = TAIL_SENTINEL;
            return;
        }

        let max_start = total_lines.saturating_sub(visible_lines);
        let current = if self.scroll_offset == TAIL_SENTINEL {
            max_start
        } else {
            self.scroll_offset.min(max_start)
        };

        let new_top = if delta < 0 {
            current.saturating_sub(delta.unsigned_abs() as usize)
        } else {
            current.saturating_add(delta as usize).min(max_start)
        };

        self.scroll_offset = if new_top >= max_start {
            TAIL_SENTINEL
        } else {
            new_top
        };
    }
}
