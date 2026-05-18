use ratatui::style::Color;

// ── Visual markers (from DeepSeek-TUI pattern) ──
pub(super) const USER_GLYPH: &str = "\u{258E}";       // ▎
pub(super) const ASSISTANT_GLYPH: &str = "\u{25CF}";  // ●
pub(super) const RAIL: &str = "\u{258F} ";            // ▏ + space
pub(super) const THINKING_RAIL: &str = "\u{254E} ";   // ╎ + space
pub(super) const TOOL_GLYPH: &str = "\u{25B8} ";      // ▸
pub(super) const CARD_TOP: &str = "\u{256D} ";        // ╭
pub(super) const CARD_MID: &str = "\u{2502} ";        // │
pub(super) const CARD_BOT: &str = "\u{2570} ";        // ╰
pub(super) const PROMPT: &str = "\u{276F} ";          // ❯

// ── Palette ──
pub(super) const COLOR_USER: Color = Color::Cyan;
pub(super) const COLOR_ASSISTANT: Color = Color::Green;
pub(super) const COLOR_THINKING: Color = Color::DarkGray;
pub(super) const COLOR_TOOL: Color = Color::Yellow;
pub(super) const COLOR_SYSTEM: Color = Color::DarkGray;
pub(super) const COLOR_RAIL: Color = Color::DarkGray;
pub(super) const COLOR_INPUT: Color = Color::White;
pub(super) const COLOR_PROMPT: Color = Color::Green;
