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
    settings: JsonColorTheme,
}

impl JsonHighlighter {
    fn token_color(&self, token: JsonToken) -> Color {
        match token {
            JsonToken::Key => self.settings.key,
            JsonToken::String => self.settings.string,
            JsonToken::Number => self.settings.number,
            JsonToken::Boolean => self.settings.boolean,
            JsonToken::Null => self.settings.null,
            JsonToken::Punctuation => self.settings.punctuation,
            JsonToken::Whitespace => self.settings.text,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JsonColorTheme {
    pub key: Color,
    pub string: Color,
    pub number: Color,
    pub boolean: Color,
    pub null: Color,
    pub punctuation: Color,
    pub text: Color,
}

impl JsonColorTheme {
    pub fn default_dark() -> Self {
        Self {
            key: Color::from_rgb(0.4, 0.76, 0.94), // Light blue for keys
            string: Color::from_rgb(0.73, 0.87, 0.53), // Light green for strings
            number: Color::from_rgb(0.88, 0.73, 0.53), // Orange for numbers
            boolean: Color::from_rgb(0.86, 0.47, 0.65), // Pink for booleans
            null: Color::from_rgb(0.86, 0.47, 0.65), // Pink for null
            punctuation: Color::from_rgb(0.8, 0.8, 0.8), // Light gray
            text: Color::WHITE,
        }
    }

    pub fn default_light() -> Self {
        Self {
            key: Color::from_rgb(0.0, 0.33, 0.8),        // Blue for keys
            string: Color::from_rgb(0.13, 0.54, 0.13),   // Green for strings
            number: Color::from_rgb(0.8, 0.4, 0.0),      // Orange for numbers
            boolean: Color::from_rgb(0.6, 0.0, 0.6),     // Purple for booleans
            null: Color::from_rgb(0.6, 0.0, 0.6),        // Purple for null
            punctuation: Color::from_rgb(0.3, 0.3, 0.3), // Dark gray
            text: Color::BLACK,
        }
    }

    pub fn vscode_dark() -> Self {
        Self {
            key: Color::from_rgb(0.61, 0.82, 0.96),         // VS Code blue
            string: Color::from_rgb(0.81, 0.71, 0.58),      // VS Code beige
            number: Color::from_rgb(0.71, 0.86, 0.65),      // VS Code light green
            boolean: Color::from_rgb(0.34, 0.63, 0.83),     // VS Code blue
            null: Color::from_rgb(0.34, 0.63, 0.83),        // VS Code blue
            punctuation: Color::from_rgb(0.85, 0.85, 0.85), // Light gray
            text: Color::from_rgb(0.85, 0.85, 0.85),
        }
    }
}

impl text::Highlighter for JsonHighlighter {
    type Settings = JsonColorTheme;
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
        let mut highlights = Vec::new();
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        let mut current_context_is_key = true;

        while i < chars.len() {
            let ch = chars[i];
            let start = i;

            match ch {
                '"' => {
                    // Find the end of the string
                    i += 1;
                    while i < chars.len() {
                        if chars[i] == '\\' && i + 1 < chars.len() {
                            i += 2; // Skip escaped character
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
                    // Parse number
                    while i < chars.len()
                        && (chars[i].is_ascii_digit()
                            || chars[i] == '.'
                            || chars[i] == '-'
                            || chars[i] == 'e'
                            || chars[i] == 'E'
                            || chars[i] == '+')
                    {
                        i += 1;
                    }
                    highlights.push((start..i, self.token_color(JsonToken::Number)));
                }
                't' | 'f' if i + 4 <= chars.len() => {
                    let word: String = chars[i..std::cmp::min(i + 5, chars.len())].iter().collect();
                    if word.starts_with("true") {
                        highlights.push((start..i + 4, self.token_color(JsonToken::Boolean)));
                        i += 4;
                    } else if word.starts_with("false") {
                        highlights.push((start..i + 5, self.token_color(JsonToken::Boolean)));
                        i += 5;
                    } else {
                        highlights.push((start..i + 1, self.token_color(JsonToken::Whitespace)));
                        i += 1;
                    }
                }
                'n' if i + 4 <= chars.len() => {
                    let word: String = chars[i..i + 4].iter().collect();
                    if word == "null" {
                        highlights.push((start..i + 4, self.token_color(JsonToken::Null)));
                        i += 4;
                    } else {
                        highlights.push((start..i + 1, self.token_color(JsonToken::Whitespace)));
                        i += 1;
                    }
                }
                _ => {
                    // Whitespace or other
                    highlights.push((start..i + 1, self.token_color(JsonToken::Whitespace)));
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

// Usage example:
// text_editor(&self.current_tab().response_body_content)
//     .on_action(Message::ResponseBodyAction)
//     .highlight::<JsonHighlighter>(JsonColorTheme::default_dark())
