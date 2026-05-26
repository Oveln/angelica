use ratatui::style::Color;

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub(super) user: Color,
    pub(super) assistant: Color,
    pub(super) thinking: Color,
    pub(super) tool: Color,
    pub(super) system: Color,
    pub(super) rail: Color,
    pub(super) input: Color,
    pub(super) prompt: Color,
    pub(super) error: Color,
    pub(super) success: Color,
    pub(super) warning: Color,
    pub(super) border: Color,
    pub(super) border_active: Color,
    pub(super) muted: Color,
    pub(super) accent: Color,
    pub(super) diff_added_bg: Color,
    pub(super) diff_removed_bg: Color,
    pub(super) diff_context_bg: Color,
    pub(super) diff_hunk: Color,
    pub(super) panel_bg: Color,
    pub(super) selection_bg: Color,
    pub(super) status_bg: Color,
    pub(super) status_fg: Color,
    pub(super) status_muted: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            user: Color::Rgb(137, 221, 255),
            assistant: Color::Rgb(195, 232, 141),
            thinking: Color::Rgb(92, 99, 112),
            tool: Color::Rgb(255, 214, 102),
            system: Color::Rgb(92, 99, 112),
            rail: Color::Rgb(64, 69, 82),
            input: Color::Rgb(220, 223, 228),
            prompt: Color::Rgb(195, 232, 141),
            error: Color::Rgb(255, 115, 112),
            success: Color::Rgb(195, 232, 141),
            warning: Color::Rgb(255, 214, 102),
            border: Color::Rgb(50, 54, 65),
            border_active: Color::Rgb(137, 221, 255),
            muted: Color::Rgb(92, 99, 112),
            accent: Color::Rgb(187, 154, 247),
            diff_added_bg: Color::Rgb(32, 48, 59),
            diff_removed_bg: Color::Rgb(55, 34, 44),
            diff_context_bg: Color::Rgb(35, 38, 48),
            diff_hunk: Color::Rgb(255, 214, 102),
            panel_bg: Color::Rgb(35, 38, 48),
            selection_bg: Color::Rgb(55, 60, 75),
            status_bg: Color::Rgb(30, 33, 40),
            status_fg: Color::Rgb(180, 184, 194),
            status_muted: Color::Rgb(100, 106, 120),
        }
    }
}

pub(super) const USER_GLYPH: &str = "\u{258E}";
pub(super) const ASSISTANT_GLYPH: &str = "\u{25CF}";
pub(super) const RAIL: &str = "\u{258F} ";
pub(super) const THINKING_RAIL: &str = "\u{254E} ";
pub(super) const TOOL_GLYPH: &str = "\u{25B8} ";
pub(super) const CARD_TOP: &str = "\u{256D} ";
pub(super) const CARD_MID: &str = "\u{2502} ";
pub(super) const CARD_BOT: &str = "\u{2570} ";
pub(super) const PROMPT: &str = "\u{276F} ";

pub(super) const APP_NAME: &str = "angelica";
pub(super) const APP_TAGLINE: &str = "An electronic ghost";

pub fn logo_lines() -> Vec<&'static str> {
    vec![
        "   ░███                                     ░██ ░██                      ",
        "  ░██░██                                    ░██                          ",
        " ░██  ░██  ░████████   ░████████  ░███████  ░██ ░██ ░███████   ░██████   ",
        "░█████████ ░██    ░██ ░██    ░██ ░██    ░██ ░██ ░██░██    ░██       ░██  ",
        "░██    ░██ ░██    ░██ ░██    ░██ ░█████████ ░██ ░██░██         ░███████  ",
        "░██    ░██ ░██    ░██ ░██   ░███ ░██        ░██ ░██░██    ░██ ░██   ░██  ",
        "░██    ░██ ░██    ░██  ░█████░██  ░███████  ░██ ░██ ░███████   ░█████░██ ",
        "                             ░██                                          ",
        "                       ░███████                                          ",
    ]
}
