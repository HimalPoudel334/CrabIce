//! Find / replace search logic over the active editor.

use crate::app::CrabiPie;

impl CrabiPie {
    pub(crate) fn position_to_line_col(text: &str, byte_pos: usize) -> (usize, usize) {
        let mut line_idx = 0;
        let mut line_start_byte = 0;

        for line in text.split_inclusive('\n') {
            let line_end_byte = line_start_byte + line.len();

            if byte_pos >= line_start_byte && byte_pos < line_end_byte {
                let byte_offset_in_line = byte_pos - line_start_byte;

                // Convert byte offset → char offset
                let char_offset = line[..byte_offset_in_line].chars().count();

                return (line_idx, char_offset);
            }

            line_start_byte = line_end_byte;
            line_idx += 1;
        }

        (0, 0)
    }

    pub(crate) fn find_matches(&self, text: &str, pattern: &str) -> Vec<usize> {
        if pattern.is_empty() {
            return Vec::new();
        }

        let mut matches = Vec::new();

        if self.case_sensitive {
            if self.whole_word {
                // Case sensitive + whole word
                let mut start = 0;
                while let Some(pos) = text[start..].find(pattern) {
                    let abs_pos = start + pos;
                    let before =
                        abs_pos == 0 || !text[..abs_pos].chars().last().unwrap().is_alphanumeric();
                    let after_pos = abs_pos + pattern.len();
                    let after = after_pos >= text.len()
                        || !text[after_pos..].chars().next().unwrap().is_alphanumeric();

                    if before && after {
                        matches.push(abs_pos);
                    }
                    start = abs_pos + 1;
                }
            } else {
                // Case sensitive only
                let mut start = 0;
                while let Some(pos) = text[start..].find(pattern) {
                    matches.push(start + pos);
                    start += pos + 1;
                }
            }
        } else {
            let text_lower = text.to_lowercase();
            let pattern_lower = pattern.to_lowercase();

            if self.whole_word {
                // Case insensitive + whole word
                let mut start = 0;
                while let Some(pos) = text_lower[start..].find(&pattern_lower) {
                    let abs_pos = start + pos;
                    let before =
                        abs_pos == 0 || !text[..abs_pos].chars().last().unwrap().is_alphanumeric();
                    let after_pos = abs_pos + pattern.len();
                    let after = after_pos >= text.len()
                        || !text[after_pos..].chars().next().unwrap().is_alphanumeric();

                    if before && after {
                        matches.push(abs_pos);
                    }
                    start = abs_pos + 1;
                }
            } else {
                // Case insensitive only
                let mut start = 0;
                while let Some(pos) = text_lower[start..].find(&pattern_lower) {
                    matches.push(start + pos);
                    start += pos + 1;
                }
            }
        }

        matches
    }

    pub(crate) fn find_next(&mut self) {
        if !self.find_dialog_open {
            return;
        }

        let text = self
            .current_tab()
            .map(|t| t.response_body.clone())
            .unwrap_or_default();

        println!("=== FIND NEXT ===");
        println!("Text length: {}", text.len());
        println!("Number of lines: {}", text.lines().count());
        println!(
            "First 200 chars: {:?}",
            &text.chars().take(200).collect::<String>()
        );

        let matches = self.find_matches(&text, &self.find_text);

        println!("Search text: '{}'", self.find_text);
        println!("Found {} matches", matches.len());
        println!("Match positions: {:?}", matches);

        self.total_matches = matches.len();

        if matches.is_empty() {
            self.current_match = 0;
            self.current_match_pos = None;
            self.search_match_positions = Vec::new();
            self.current_match_line_col = None;
            self.search_match_length = 0;
            return;
        }

        // Convert all matches to line/col positions
        let match_positions: Vec<(usize, usize)> = matches
            .iter()
            .map(|&pos| Self::position_to_line_col(&text, pos))
            .collect();

        // println!("Line/col positions: {:?}", match_positions);

        // Update search state
        self.search_match_positions = match_positions;
        self.search_match_length = self.find_text.chars().count();

        println!("Match length (chars): {}", self.search_match_length);

        // Move to next match
        if self.current_match == 0 || self.current_match >= matches.len() {
            self.current_match = 1;
        } else {
            self.current_match += 1;
        }

        if self.current_match > matches.len() {
            self.current_match = 1;
        }

        if self.current_match > 0 {
            let match_pos = matches[self.current_match - 1];
            self.current_match_pos = Some(match_pos);
            self.current_match_line_col = Some(self.search_match_positions[self.current_match - 1]);
        }
    }

    pub(crate) fn find_previous(&mut self) {
        if !self.find_dialog_open {
            return;
        }

        let text = self
            .current_tab()
            .map(|t| t.response_body.clone())
            .unwrap_or_default();
        let matches = self.find_matches(&text, &self.find_text);

        self.total_matches = matches.len();

        if matches.is_empty() {
            self.current_match = 0;
            self.current_match_pos = None;
            self.search_match_positions = Vec::new();
            self.current_match_line_col = None;
            self.search_match_length = 0;
            return;
        }

        // Convert all matches to line/col positions
        let match_positions: Vec<(usize, usize)> = matches
            .iter()
            .map(|&pos| Self::position_to_line_col(&text, pos))
            .collect();

        // Update search state
        self.search_match_positions = match_positions;
        self.search_match_length = self.find_text.chars().count();

        // Move to previous match
        if self.current_match <= 1 {
            self.current_match = matches.len();
        } else {
            self.current_match -= 1;
        }

        if self.current_match > 0 {
            let match_pos = matches[self.current_match - 1];
            self.current_match_pos = Some(match_pos);
            self.current_match_line_col = Some(self.search_match_positions[self.current_match - 1]);
        }
    }
}
