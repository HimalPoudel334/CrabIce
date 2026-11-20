//! Theme name <-> theme value conversion.

use crate::ui::json_highlighter;

pub(crate) fn theme_from_str(s: &str) -> iced::Theme {
    match s {
        "Light" => iced::Theme::Light,
        "Dark" => iced::Theme::Dark,
        "Dracula" => iced::Theme::Dracula,
        "Nord" => iced::Theme::Nord,
        "Solarized Light" => iced::Theme::SolarizedLight,
        "Solarized Dark" => iced::Theme::SolarizedDark,
        "Gruvbox Light" => iced::Theme::GruvboxLight,
        "Gruvbox Dark" => iced::Theme::GruvboxDark,
        "Catppuccin Latte" => iced::Theme::CatppuccinLatte,
        "Catppuccin Frappe" => iced::Theme::CatppuccinFrappe,
        "Catppuccin Macchiato" => iced::Theme::CatppuccinMacchiato,
        "Catppuccin Mocha" => iced::Theme::CatppuccinMocha,
        "Tokyo Night" => iced::Theme::TokyoNight,
        "Tokyo Night Storm" => iced::Theme::TokyoNightStorm,
        "Tokyo Night Light" => iced::Theme::TokyoNightLight,
        "Kanagawa Wave" => iced::Theme::KanagawaWave,
        "Kanagawa Dragon" => iced::Theme::KanagawaDragon,
        "Kanagawa Lotus" => iced::Theme::KanagawaLotus,
        "Moonfly" => iced::Theme::Moonfly,
        "Nightfly" => iced::Theme::Nightfly,
        "Oxocarbon" => iced::Theme::Oxocarbon,
        "Ferra" => iced::Theme::Ferra,
        _ => iced::Theme::CatppuccinMocha, // default fallback
    }
}

pub(crate) fn json_theme_from_str(s: &str) -> json_highlighter::JsonThemeWrapper {
    match s {
        "Base 16 Eighties" => {
            json_highlighter::JsonThemeWrapper::Builtin(iced::highlighter::Theme::Base16Eighties)
        }
        "Base 16 Mocha" => {
            json_highlighter::JsonThemeWrapper::Builtin(iced::highlighter::Theme::Base16Mocha)
        }
        "Base 16 Ocean" => {
            json_highlighter::JsonThemeWrapper::Builtin(iced::highlighter::Theme::Base16Ocean)
        }
        "Solarized Dark" => {
            json_highlighter::JsonThemeWrapper::Builtin(iced::highlighter::Theme::SolarizedDark)
        }
        "Inspired GitHub" => {
            json_highlighter::JsonThemeWrapper::Builtin(iced::highlighter::Theme::InspiredGitHub)
        }
        "Default Dark" => json_highlighter::JsonThemeWrapper::Custom(
            json_highlighter::CustomJsonTheme::DEFAULT_DARK,
        ),
        "Default Light" => json_highlighter::JsonThemeWrapper::Custom(
            json_highlighter::CustomJsonTheme::DEFAULT_LIGHT,
        ),
        "VSCode Dark" => json_highlighter::JsonThemeWrapper::Custom(
            json_highlighter::CustomJsonTheme::VSCODE_DARK,
        ),
        _ => json_highlighter::JsonThemeWrapper::Custom(
            json_highlighter::CustomJsonTheme::VSCODE_DARK,
        ),
    }
}
