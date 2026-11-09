use super::geom::Color;

/// Catppuccin Mocha, matching iced's `Theme::CatppuccinMocha`.
#[derive(Clone, Debug)]
pub struct Theme {
    pub bg: Color,
    pub bar_bg: Color, // inputs / editors
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

impl Theme {
    pub const fn mocha() -> Self {
        Self {
            bg: Color::hex(0x1e1e2e),
            bar_bg: Color::hex(0x181825),
            surface: Color::hex(0x313244),
            tab_bar_bg: Color::hex(0x11111b),
            tab_active: Color::hex(0x1e1e2e),
            tab_inactive: Color::hex(0x181825),
            tab_hover: Color::hex(0x585b70),
            border: Color::hex(0x45475a),
            text: Color::hex(0xcdd6f4),
            text_dim: Color::hex(0x6c7086),
            accent: Color::hex(0x89b4fa),
            on_accent: Color::hex(0x1e1e2e),
            danger: Color::hex(0xf38ba8),
            success: Color::hex(0xa6e3a1),
            method_get: Color::hex(0xa6e3a1),
            method_post: Color::hex(0xf9e2af),
            method_put: Color::hex(0x89b4fa),
            method_patch: Color::hex(0xcba6f7),
            method_delete: Color::hex(0xf38ba8),
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::mocha()
    }
}
