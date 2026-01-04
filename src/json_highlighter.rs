use iced::Color;
use iced::widget::text;
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonToken {
    Key,
    String,
    Number,
    Boolean,
    Null,
    Punctuation,
    Whitespace,
}

pub struct JsonHighlighter {
    current_line_number: usize,
    settings: JsonThemeWrapper,
}

impl JsonHighlighter {
    fn token_color(&self, token: JsonToken) -> Color {
        match token {
            JsonToken::Key => self.settings.key_color(),
            JsonToken::String => self.settings.string_color(),
            JsonToken::Number => self.settings.number_color(),
            JsonToken::Boolean => self.settings.boolean_color(),
            JsonToken::Null => self.settings.null_color(),
            JsonToken::Punctuation => self.settings.punctuation_color(),
            JsonToken::Whitespace => self.settings.text_color(),
        }
    }
}

// Wrapper enum that combines built-in and custom themes
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JsonThemeWrapper {
    Builtin(iced::highlighter::Theme),
    Custom(CustomJsonTheme),
}

impl JsonThemeWrapper {
    pub const ALL: &'static [JsonThemeWrapper] = &[
        // Built-in themes
        JsonThemeWrapper::Builtin(iced::highlighter::Theme::Base16Eighties),
        JsonThemeWrapper::Builtin(iced::highlighter::Theme::Base16Mocha),
        JsonThemeWrapper::Builtin(iced::highlighter::Theme::Base16Ocean),
        JsonThemeWrapper::Builtin(iced::highlighter::Theme::SolarizedDark),
        JsonThemeWrapper::Builtin(iced::highlighter::Theme::InspiredGitHub),
        // Custom themes
        JsonThemeWrapper::Custom(CustomJsonTheme::DEFAULT_DARK),
        JsonThemeWrapper::Custom(CustomJsonTheme::DEFAULT_LIGHT),
        JsonThemeWrapper::Custom(CustomJsonTheme::VSCODE_DARK),
    ];

    // Helper methods to extract colors
    fn key_color(&self) -> Color {
        match self {
            JsonThemeWrapper::Builtin(theme) => Self::builtin_key_color(*theme),
            JsonThemeWrapper::Custom(custom) => custom.key,
        }
    }

    fn string_color(&self) -> Color {
        match self {
            JsonThemeWrapper::Builtin(theme) => Self::builtin_string_color(*theme),
            JsonThemeWrapper::Custom(custom) => custom.string,
        }
    }

    fn number_color(&self) -> Color {
        match self {
            JsonThemeWrapper::Builtin(theme) => Self::builtin_number_color(*theme),
            JsonThemeWrapper::Custom(custom) => custom.number,
        }
    }

    fn boolean_color(&self) -> Color {
        match self {
            JsonThemeWrapper::Builtin(theme) => Self::builtin_boolean_color(*theme),
            JsonThemeWrapper::Custom(custom) => custom.boolean,
        }
    }

    fn null_color(&self) -> Color {
        match self {
            JsonThemeWrapper::Builtin(theme) => Self::builtin_null_color(*theme),
            JsonThemeWrapper::Custom(custom) => custom.null,
        }
    }

    fn punctuation_color(&self) -> Color {
        match self {
            JsonThemeWrapper::Builtin(theme) => Self::builtin_punctuation_color(*theme),
            JsonThemeWrapper::Custom(custom) => custom.punctuation,
        }
    }

    fn text_color(&self) -> Color {
        match self {
            JsonThemeWrapper::Builtin(theme) => Self::builtin_text_color(*theme),
            JsonThemeWrapper::Custom(custom) => custom.text,
        }
    }

    // Map built-in theme colors (approximate mapping to JSON syntax)
    fn builtin_key_color(theme: iced::highlighter::Theme) -> Color {
        // Keys are like function names in code
        match theme {
            iced::highlighter::Theme::SolarizedDark => Color::from_rgb(0.51, 0.58, 0.0),
            _ => Color::from_rgb(0.4, 0.76, 0.94), // Default blue
        }
    }

    fn builtin_string_color(theme: iced::highlighter::Theme) -> Color {
        match theme {
            iced::highlighter::Theme::SolarizedDark => Color::from_rgb(0.16, 0.63, 0.6),
            _ => Color::from_rgb(0.73, 0.87, 0.53), // Default green
        }
    }

    fn builtin_number_color(theme: iced::highlighter::Theme) -> Color {
        match theme {
            iced::highlighter::Theme::SolarizedDark => Color::from_rgb(0.8, 0.29, 0.09),
            _ => Color::from_rgb(0.88, 0.73, 0.53), // Default orange
        }
    }

    fn builtin_boolean_color(theme: iced::highlighter::Theme) -> Color {
        match theme {
            iced::highlighter::Theme::SolarizedDark => Color::from_rgb(0.83, 0.21, 0.51),
            _ => Color::from_rgb(0.86, 0.47, 0.65), // Default pink
        }
    }

    fn builtin_null_color(theme: iced::highlighter::Theme) -> Color {
        Self::builtin_boolean_color(theme)
    }

    fn builtin_punctuation_color(theme: iced::highlighter::Theme) -> Color {
        match theme {
            iced::highlighter::Theme::SolarizedDark => Color::from_rgb(0.58, 0.63, 0.63),
            _ => Color::from_rgb(0.8, 0.8, 0.8),
        }
    }

    fn builtin_text_color(theme: iced::highlighter::Theme) -> Color {
        match theme {
            iced::highlighter::Theme::SolarizedDark => Color::from_rgb(0.58, 0.63, 0.63),
            _ => Color::WHITE,
        }
    }
}

impl std::fmt::Display for JsonThemeWrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JsonThemeWrapper::Builtin(theme) => write!(f, "{}", theme),
            JsonThemeWrapper::Custom(custom) => write!(f, "{}", custom),
        }
    }
}

// Custom JSON-specific themes
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CustomJsonTheme {
    pub key: Color,
    pub string: Color,
    pub number: Color,
    pub boolean: Color,
    pub null: Color,
    pub punctuation: Color,
    pub text: Color,
}

impl CustomJsonTheme {
    pub const DEFAULT_DARK: Self = Self {
        key: Color::from_rgb(0.4, 0.76, 0.94),
        string: Color::from_rgb(0.73, 0.87, 0.53),
        number: Color::from_rgb(0.88, 0.73, 0.53),
        boolean: Color::from_rgb(0.86, 0.47, 0.65),
        null: Color::from_rgb(0.86, 0.47, 0.65),
        punctuation: Color::from_rgb(0.8, 0.8, 0.8),
        text: Color::WHITE,
    };

    pub const DEFAULT_LIGHT: Self = Self {
        key: Color::from_rgb(0.0, 0.33, 0.8),
        string: Color::from_rgb(0.13, 0.54, 0.13),
        number: Color::from_rgb(0.8, 0.4, 0.0),
        boolean: Color::from_rgb(0.6, 0.0, 0.6),
        null: Color::from_rgb(0.6, 0.0, 0.6),
        punctuation: Color::from_rgb(0.3, 0.3, 0.3),
        text: Color::BLACK,
    };

    pub const VSCODE_DARK: Self = Self {
        key: Color::from_rgb(0.61, 0.82, 0.96),
        string: Color::from_rgb(0.81, 0.71, 0.58),
        number: Color::from_rgb(0.71, 0.86, 0.65),
        boolean: Color::from_rgb(0.34, 0.63, 0.83),
        null: Color::from_rgb(0.34, 0.63, 0.83),
        punctuation: Color::from_rgb(0.85, 0.85, 0.85),
        text: Color::from_rgb(0.85, 0.85, 0.85),
    };
}

impl std::fmt::Display for CustomJsonTheme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if *self == Self::DEFAULT_DARK {
            write!(f, "Custom Dark")
        } else if *self == Self::DEFAULT_LIGHT {
            write!(f, "Custom Light")
        } else if *self == Self::VSCODE_DARK {
            write!(f, "VS Code Dark")
        } else {
            write!(f, "Custom")
        }
    }
}

impl text::Highlighter for JsonHighlighter {
    type Settings = JsonThemeWrapper;
    type Highlight = Color;
    type Iterator<'a> = Box<dyn Iterator<Item = (Range<usize>, Self::Highlight)> + 'a>;

    fn new(settings: &Self::Settings) -> Self {
        Self {
            current_line_number: 0,
            settings: *settings,
        }
    }

    fn update(&mut self, new_settings: &Self::Settings) {
        self.settings = *new_settings;
    }

    fn change_line(&mut self, line: usize) {
        self.current_line_number = line;
    }

    fn highlight_line(&mut self, line: &str) -> Self::Iterator<'_> {
        // Early return for empty lines
        if line.is_empty() {
            return Box::new(std::iter::empty());
        }

        let mut highlights = Vec::with_capacity(line.len() / 10); // Pre-allocate
        let chars: Vec<char> = line.chars().collect();
        let len = chars.len();
        let mut i = 0;
        let mut current_context_is_key = true;

        while i < len {
            let ch = chars[i];
            let start = i;

            match ch {
                '"' => {
                    i += 1;
                    while i < len {
                        if chars[i] == '\\' && i + 1 < len {
                            i += 2;
                            continue;
                        }
                        if chars[i] == '"' {
                            i += 1;
                            break;
                        }
                        i += 1;
                    }

                    let token = if current_context_is_key {
                        JsonToken::Key
                    } else {
                        JsonToken::String
                    };
                    highlights.push((start..i, self.token_color(token)));
                }
                ':' => {
                    highlights.push((start..i + 1, self.token_color(JsonToken::Punctuation)));
                    current_context_is_key = false;
                    i += 1;
                }
                ',' | '{' | '}' | '[' | ']' => {
                    highlights.push((start..i + 1, self.token_color(JsonToken::Punctuation)));
                    if ch == ',' {
                        current_context_is_key = true;
                    }
                    i += 1;
                }
                c if c.is_ascii_digit() || c == '-' => {
                    while i < len && matches!(chars[i], '0'..='9' | '.' | '-' | 'e' | 'E' | '+') {
                        i += 1;
                    }
                    highlights.push((start..i, self.token_color(JsonToken::Number)));
                }
                't' if line[start..].starts_with("true") => {
                    highlights.push((start..i + 4, self.token_color(JsonToken::Boolean)));
                    i += 4;
                }
                'f' if line[start..].starts_with("false") => {
                    highlights.push((start..i + 5, self.token_color(JsonToken::Boolean)));
                    i += 5;
                }
                'n' if line[start..].starts_with("null") => {
                    highlights.push((start..i + 4, self.token_color(JsonToken::Null)));
                    i += 4;
                }
                // Skip whitespace without highlighting
                c if c.is_whitespace() => {
                    i += 1;
                    continue;
                }
                _ => {
                    i += 1;
                }
            }
        }

        Box::new(highlights.into_iter())
    }

    fn current_line(&self) -> usize {
        self.current_line_number
    }
}
