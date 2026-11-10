use super::geom::Color;

/// Colors for one app theme. Built from a small palette so adding a theme is one line.
#[derive(Clone, Debug)]
pub struct Theme {
    pub name: &'static str,
    pub bg: Color,
    pub bar_bg: Color,  // inputs / editors
    pub surface: Color, // pickers, plain buttons
    pub tab_bar_bg: Color,
    pub tab_active: Color,
    pub tab_inactive: Color,
    pub tab_hover: Color,
    pub border: Color,
    pub text: Color,
    pub text_dim: Color,
    pub accent: Color,
    pub on_accent: Color,
    pub danger: Color,
    pub success: Color,
    pub method_get: Color,
    pub method_post: Color,
    pub method_put: Color,
    pub method_patch: Color,
    pub method_delete: Color,
}

struct Palette {
    bg: u32,
    input: u32,
    surface: u32,
    border: u32,
    crust: u32,
    text: u32,
    dim: u32,
    accent: u32,
    danger: u32,
    success: u32,
    warn: u32,
    purple: u32,
}

const fn build(name: &'static str, p: Palette) -> Theme {
    Theme {
        name,
        bg: Color::hex(p.bg),
        bar_bg: Color::hex(p.input),
        surface: Color::hex(p.surface),
        tab_bar_bg: Color::hex(p.crust),
        tab_active: Color::hex(p.bg),
        tab_inactive: Color::hex(p.input),
        tab_hover: Color::hex(p.border),
        border: Color::hex(p.border),
        text: Color::hex(p.text),
        text_dim: Color::hex(p.dim),
        accent: Color::hex(p.accent),
        on_accent: Color::hex(p.bg),
        danger: Color::hex(p.danger),
        success: Color::hex(p.success),
        method_get: Color::hex(p.success),
        method_post: Color::hex(p.warn),
        method_put: Color::hex(p.accent),
        method_patch: Color::hex(p.purple),
        method_delete: Color::hex(p.danger),
    }
}

impl Theme {
    /// Dark themes only: the JSON syntax colors are tuned for dark backgrounds.
    pub const NAMES: [&'static str; 8] = [
        "Catppuccin Mocha",
        "Catppuccin Macchiato",
        "Catppuccin Frappe",
        "Dracula",
        "Nord",
        "Tokyo Night",
        "Gruvbox Dark",
        "One Dark",
    ];

    pub fn named(name: &str) -> Theme {
        match name {
            "Catppuccin Macchiato" => build("Catppuccin Macchiato", Palette { bg: 0x24273a, input: 0x1e2030, surface: 0x363a4f, border: 0x494d64, crust: 0x181926, text: 0xcad3f5, dim: 0x6e738d, accent: 0x8aadf4, danger: 0xed8796, success: 0xa6da95, warn: 0xeed49f, purple: 0xc6a0f6 }),
            "Catppuccin Frappe" => build("Catppuccin Frappe", Palette { bg: 0x303446, input: 0x292c3c, surface: 0x414559, border: 0x51576d, crust: 0x232634, text: 0xc6d0f5, dim: 0x737994, accent: 0x8caaee, danger: 0xe78284, success: 0xa6d189, warn: 0xe5c890, purple: 0xca9ee6 }),
            "Dracula" => build("Dracula", Palette { bg: 0x282a36, input: 0x21222c, surface: 0x343746, border: 0x44475a, crust: 0x191a21, text: 0xf8f8f2, dim: 0x6272a4, accent: 0xbd93f9, danger: 0xff5555, success: 0x50fa7b, warn: 0xf1fa8c, purple: 0xff79c6 }),
            "Nord" => build("Nord", Palette { bg: 0x2e3440, input: 0x272c36, surface: 0x3b4252, border: 0x4c566a, crust: 0x20242c, text: 0xd8dee9, dim: 0x7b88a1, accent: 0x88c0d0, danger: 0xbf616a, success: 0xa3be8c, warn: 0xebcb8b, purple: 0xb48ead }),
            "Tokyo Night" => build("Tokyo Night", Palette { bg: 0x1a1b26, input: 0x16161e, surface: 0x24283b, border: 0x3b4261, crust: 0x101014, text: 0xc0caf5, dim: 0x565f89, accent: 0x7aa2f7, danger: 0xf7768e, success: 0x9ece6a, warn: 0xe0af68, purple: 0xbb9af7 }),
            "Gruvbox Dark" => build("Gruvbox Dark", Palette { bg: 0x282828, input: 0x1d2021, surface: 0x3c3836, border: 0x504945, crust: 0x141617, text: 0xebdbb2, dim: 0x928374, accent: 0x83a598, danger: 0xfb4934, success: 0xb8bb26, warn: 0xfabd2f, purple: 0xd3869b }),
            "One Dark" => build("One Dark", Palette { bg: 0x282c34, input: 0x21252b, surface: 0x2c313a, border: 0x3e4451, crust: 0x181a1f, text: 0xabb2bf, dim: 0x5c6370, accent: 0x61afef, danger: 0xe06c75, success: 0x98c379, warn: 0xe5c07b, purple: 0xc678dd }),
            _ => Self::mocha(),
        }
    }

    pub const fn mocha() -> Self {
        build(
            "Catppuccin Mocha",
            Palette { bg: 0x1e1e2e, input: 0x181825, surface: 0x313244, border: 0x45475a, crust: 0x11111b, text: 0xcdd6f4, dim: 0x6c7086, accent: 0x89b4fa, danger: 0xf38ba8, success: 0xa6e3a1, warn: 0xf9e2af, purple: 0xcba6f7 },
        )
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::mocha()
    }
}
