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

#[derive(Debug, Clone, Copy)]
pub enum HighlightType {
    Syntax(Color),
    SearchMatch,
    CurrentMatch,
}

// Settings that include both theme and search information
#[derive(Debug, Clone, PartialEq)]
pub struct JsonHighlighterSettings {
    pub theme: JsonThemeWrapper,
    pub search_matches: Vec<(usize, usize)>,
    pub current_match: Option<(usize, usize)>,
    pub match_length: usize,
}

impl JsonHighlighterSettings {
    pub fn new(theme: JsonThemeWrapper) -> Self {
        Self {
            theme,
            search_matches: Vec::new(),
            current_match: None,
            match_length: 0,
        }
    }

    pub fn with_search(
        mut self,
        matches: Vec<(usize, usize)>,
        current: Option<(usize, usize)>,
        length: usize,
    ) -> Self {
        self.search_matches = matches;
        self.current_match = current;
        self.match_length = length;
        self
    }
}

pub struct JsonHighlighter {
    current_line_number: usize,
    settings: JsonHighlighterSettings,
}

impl JsonHighlighter {
    fn token_color(&self, token: JsonToken) -> Color {
        match token {
            JsonToken::Key => self.settings.theme.key_color(),
            JsonToken::String => self.settings.theme.string_color(),
            JsonToken::Number => self.settings.theme.number_color(),
            JsonToken::Boolean => self.settings.theme.boolean_color(),
            JsonToken::Null => self.settings.theme.null_color(),
            JsonToken::Punctuation => self.settings.theme.punctuation_color(),
            JsonToken::Whitespace => self.settings.theme.text_color(),
        }
    }

    fn apply_search_highlight(
        &self,
        highlights: &mut Vec<(Range<usize>, HighlightType)>,
        range: Range<usize>,
        highlight_type: HighlightType,
    ) {
        let mut new_highlights: Vec<(Range<usize>, HighlightType)> = Vec::new();
        let mut covered = false;

        for (existing_range, existing_type) in highlights.drain(..) {
            if existing_range.end <= range.start || existing_range.start >= range.end {
                // No overlap - keep existing highlight
                new_highlights.push((existing_range, existing_type));
            } else {
                // There's overlap
                covered = true;

                // Part before overlap (keep original syntax highlighting)
                if existing_range.start < range.start {
                    new_highlights.push((existing_range.start..range.start, existing_type));
                }

                // Overlapping part gets search highlight
                let overlap_start = existing_range.start.max(range.start);
                let overlap_end = existing_range.end.min(range.end);
                new_highlights.push((overlap_start..overlap_end, highlight_type));

                // Part after overlap (keep original syntax highlighting)
                if existing_range.end > range.end {
                    new_highlights.push((range.end..existing_range.end, existing_type));
                }
            }
        }

        // If no existing highlight covered this range, add it with default color
        if !covered {
            new_highlights.push((range, highlight_type));
        }

        *highlights = new_highlights;
    }
}

impl text::Highlighter for JsonHighlighter {
    type Settings = JsonHighlighterSettings;
    type Highlight = HighlightType;
    type Iterator<'a> = Box<dyn Iterator<Item = (Range<usize>, Self::Highlight)> + 'a>;

    fn new(settings: &Self::Settings) -> Self {
        Self {
            current_line_number: 0,
            settings: settings.clone(),
        }
    }

    fn update(&mut self, new_settings: &Self::Settings) {
        self.settings = new_settings.clone();
    }

    fn change_line(&mut self, line: usize) {
        println!("Changed line called with line number {line}");
        self.current_line_number = line;
    }

    fn highlight_line(&mut self, line: &str) -> Self::Iterator<'_> {
        let actual_line = self.current_line_number;

        if line.is_empty() {
            return Box::new(std::iter::empty());
        }

        let mut highlights: Vec<(Range<usize>, HighlightType)> = Vec::new();
        let chars: Vec<char> = line.chars().collect();
        let len = chars.len();
        let mut i = 0;
        let mut current_context_is_key = true;

        // --- STEP 1: Syntax Highlighting First ---
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
                    highlights.push((start..i, HighlightType::Syntax(self.token_color(token))));
                }
                ':' => {
                    highlights.push((
                        start..i + 1,
                        HighlightType::Syntax(self.token_color(JsonToken::Punctuation)),
                    ));
                    current_context_is_key = false;
                    i += 1;
                }
                ',' | '{' | '}' | '[' | ']' => {
                    highlights.push((
                        start..i + 1,
                        HighlightType::Syntax(self.token_color(JsonToken::Punctuation)),
                    ));
                    if ch == ',' {
                        current_context_is_key = true;
                    }
                    i += 1;
                }
                // ... (rest of your number/bool/null logic) ...
                c if c.is_whitespace() => {
                    i += 1;
                }
                _ => {
                    i += 1;
                }
            }
        }

        for &(line_num, col_start) in &self.settings.search_matches {
            if line_num == actual_line {
                let col_end = col_start + self.settings.match_length;
                if col_end <= len {
                    self.apply_search_highlight(
                        &mut highlights,
                        col_start..col_end,
                        HighlightType::SearchMatch,
                    );
                }
            }
        }

        if let Some((line_num, col_start)) = self.settings.current_match {
            if line_num == actual_line {
                let col_end = col_start + self.settings.match_length;
                if col_end <= len {
                    self.apply_search_highlight(
                        &mut highlights,
                        col_start..col_end,
                        HighlightType::CurrentMatch,
                    );
                }
            }
        }

        highlights.sort_by_key(|(range, _)| range.start);

        Box::new(highlights.into_iter())
    }

    fn current_line(&self) -> usize {
        self.current_line_number
    }
}

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
        match theme {
            iced::highlighter::Theme::SolarizedDark => Color::from_rgb(0.51, 0.58, 0.0),
            _ => Color::from_rgb(0.4, 0.76, 0.94),
        }
    }

    fn builtin_string_color(theme: iced::highlighter::Theme) -> Color {
        match theme {
            iced::highlighter::Theme::SolarizedDark => Color::from_rgb(0.16, 0.63, 0.6),
            _ => Color::from_rgb(0.73, 0.87, 0.53),
        }
    }

    fn builtin_number_color(theme: iced::highlighter::Theme) -> Color {
        match theme {
            iced::highlighter::Theme::SolarizedDark => Color::from_rgb(0.8, 0.29, 0.09),
            _ => Color::from_rgb(0.88, 0.73, 0.53),
        }
    }

    fn builtin_boolean_color(theme: iced::highlighter::Theme) -> Color {
        match theme {
            iced::highlighter::Theme::SolarizedDark => Color::from_rgb(0.83, 0.21, 0.51),
            _ => Color::from_rgb(0.86, 0.47, 0.65),
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
