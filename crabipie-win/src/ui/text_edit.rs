//! Single-line text editing model: caret, selection, word movement, undo.
//! Offsets are byte indices that always sit on `char` boundaries. (Combining
//! marks / emoji ZWJ sequences count as several chars: fine for URLs, header
//! values and JSON; revisit when the multi-line editor lands.)

const MAX_UNDO: usize = 200;

#[derive(Clone, Debug)]
struct Snap {
    text: String,
    caret: usize,
    anchor: usize,
}

#[derive(Clone, Debug, Default)]
pub struct LineEdit {
    text: String,
    caret: usize,
    anchor: usize,
    undo: Vec<Snap>,
    redo: Vec<Snap>,
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// A single line cannot contain line breaks; tabs become spaces.
fn sanitize(s: &str) -> String {
    s.chars()
        .filter(|c| *c != '\r' && *c != '\n')
        .map(|c| if c == '\t' { ' ' } else { c })
        .collect()
}

impl LineEdit {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn caret(&self) -> usize {
        self.caret
    }
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Programmatic replace (e.g. loading a saved URL). Clears undo history.
    pub fn set_text(&mut self, s: &str) {
        self.text = sanitize(s);
        self.caret = self.text.len();
        self.anchor = self.caret;
        self.undo.clear();
        self.redo.clear();
    }

    pub fn selection(&self) -> Option<(usize, usize)> {
        (self.caret != self.anchor)
            .then(|| (self.caret.min(self.anchor), self.caret.max(self.anchor)))
    }

    pub fn selected_text(&self) -> Option<&str> {
        self.selection().map(|(a, b)| &self.text[a..b])
    }

    // ── helpers ─────────────────────────────────────────────────────

    fn clamp(&self, mut i: usize) -> usize {
        i = i.min(self.text.len());
        while !self.text.is_char_boundary(i) {
            i -= 1;
        }
        i
    }

    fn prev_boundary(&self, i: usize) -> usize {
        self.text[..i].char_indices().next_back().map(|(p, _)| p).unwrap_or(0)
    }

    fn next_boundary(&self, i: usize) -> usize {
        self.text[i..].chars().next().map(|c| i + c.len_utf8()).unwrap_or(self.text.len())
    }

    /// Start of the word before `i` (skips separators first, then the word).
    fn prev_word(&self, mut i: usize) -> usize {
        while i > 0 {
            let q = self.prev_boundary(i);
            if is_word(self.text[q..].chars().next().unwrap()) {
                break;
            }
            i = q;
        }
        while i > 0 {
            let q = self.prev_boundary(i);
            if !is_word(self.text[q..].chars().next().unwrap()) {
                break;
            }
            i = q;
        }
        i
    }

    /// Start of the next word after `i` (skips the current word, then separators).
    fn next_word(&self, mut i: usize) -> usize {
        let len = self.text.len();
        while i < len && is_word(self.text[i..].chars().next().unwrap()) {
            i = self.next_boundary(i);
        }
        while i < len && !is_word(self.text[i..].chars().next().unwrap()) {
            i = self.next_boundary(i);
        }
        i
    }

    fn snap(&self) -> Snap {
        Snap { text: self.text.clone(), caret: self.caret, anchor: self.anchor }
    }

    fn record(&mut self) {
        let s = self.snap();
        self.undo.push(s);
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn restore(&mut self, s: Snap) {
        self.text = s.text;
        self.caret = s.caret;
        self.anchor = s.anchor;
    }

    fn remove_selection(&mut self) -> bool {
        if let Some((a, b)) = self.selection() {
            self.text.replace_range(a..b, "");
            self.caret = a;
            self.anchor = a;
            true
        } else {
            false
        }
    }

    fn move_to(&mut self, idx: usize, extend: bool) {
        self.caret = idx;
        if !extend {
            self.anchor = idx;
        }
    }

    // ── editing ─────────────────────────────────────────────────────

    pub fn insert(&mut self, s: &str) -> bool {
        let s = sanitize(s);
        if s.is_empty() && self.selection().is_none() {
            return false;
        }
        self.record();
        self.remove_selection();
        self.text.insert_str(self.caret, &s);
        self.caret += s.len();
        self.anchor = self.caret;
        true
    }

    pub fn backspace(&mut self, word: bool) -> bool {
        if self.selection().is_some() {
            self.record();
            return self.remove_selection();
        }
        if self.caret == 0 {
            return false;
        }
        self.record();
        let start = if word { self.prev_word(self.caret) } else { self.prev_boundary(self.caret) };
        self.text.replace_range(start..self.caret, "");
        self.caret = start;
        self.anchor = start;
        true
    }

    pub fn delete(&mut self, word: bool) -> bool {
        if self.selection().is_some() {
            self.record();
            return self.remove_selection();
        }
        if self.caret >= self.text.len() {
            return false;
        }
        self.record();
        let end = if word { self.next_word(self.caret) } else { self.next_boundary(self.caret) };
        self.text.replace_range(self.caret..end, "");
        true
    }

    pub fn cut(&mut self) -> Option<String> {
        let t = self.copy()?;
        self.record();
        self.remove_selection();
        Some(t)
    }

    pub fn copy(&self) -> Option<String> {
        self.selected_text().map(String::from)
    }

    pub fn undo(&mut self) -> bool {
        let Some(prev) = self.undo.pop() else { return false };
        let cur = self.snap();
        self.redo.push(cur);
        self.restore(prev);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else { return false };
        let cur = self.snap();
        self.undo.push(cur);
        self.restore(next);
        true
    }

    // ── movement / selection ────────────────────────────────────────

    pub fn move_left(&mut self, extend: bool, word: bool) {
        if !extend {
            if let Some((a, _)) = self.selection() {
                self.move_to(a, false);
                return;
            }
        }
        let to = if word { self.prev_word(self.caret) } else { self.prev_boundary(self.caret) };
        self.move_to(to, extend);
    }

    pub fn move_right(&mut self, extend: bool, word: bool) {
        if !extend {
            if let Some((_, b)) = self.selection() {
                self.move_to(b, false);
                return;
            }
        }
        let to = if word { self.next_word(self.caret) } else { self.next_boundary(self.caret) };
        self.move_to(to, extend);
    }

    pub fn home(&mut self, extend: bool) {
        self.move_to(0, extend);
    }

    pub fn end(&mut self, extend: bool) {
        self.move_to(self.text.len(), extend);
    }

    pub fn set_caret(&mut self, idx: usize, extend: bool) {
        let idx = self.clamp(idx);
        self.move_to(idx, extend);
    }

    pub fn select_all(&mut self) {
        self.anchor = 0;
        self.caret = self.text.len();
    }

    /// Selects the run of same-kind characters (word / spaces / punctuation) at `idx`.
    pub fn select_word_at(&mut self, idx: usize) {
        if self.text.is_empty() {
            return;
        }
        let idx = self.clamp(idx);
        let probe = if idx >= self.text.len() { self.prev_boundary(idx) } else { idx };
        let kind = |c: char| -> u8 {
            if is_word(c) { 0 } else if c.is_whitespace() { 1 } else { 2 }
        };
        let k = kind(self.text[probe..].chars().next().unwrap());
        let mut a = probe;
        while a > 0 {
            let q = self.prev_boundary(a);
            if kind(self.text[q..].chars().next().unwrap()) != k {
                break;
            }
            a = q;
        }
        let mut b = probe;
        while b < self.text.len() {
            if kind(self.text[b..].chars().next().unwrap()) != k {
                break;
            }
            b = self.next_boundary(b);
        }
        self.anchor = a;
        self.caret = b;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ed(s: &str) -> LineEdit {
        let mut e = LineEdit::new();
        e.set_text(s);
        e
    }

    #[test]
    fn insert_replaces_selection_and_strips_newlines() {
        let mut e = ed("hello world");
        e.set_caret(0, false);
        e.set_caret(5, true);
        assert!(e.insert("HEY\r\n"));
        assert_eq!(e.text(), "HEY world");
        assert_eq!(e.caret(), 3);
        assert!(!e.insert("\n")); // nothing left after sanitizing
    }

    #[test]
    fn backspace_delete_char_and_word() {
        let mut e = ed("foo bar baz");
        assert!(e.backspace(true));
        assert_eq!(e.text(), "foo bar ");
        assert!(e.backspace(false));
        assert_eq!(e.text(), "foo bar");
        e.home(false);
        assert!(e.delete(true));
        assert_eq!(e.text(), "bar");
        assert!(!{ e.end(false); e.delete(false) });
        e.home(false);
        assert!(!e.backspace(false));
    }

    #[test]
    fn multibyte_text_never_splits_a_char() {
        let mut e = ed("héllo 日本語");
        e.backspace(false);
        assert_eq!(e.text(), "héllo 日本");
        e.home(false);
        e.move_right(false, false);
        e.move_right(false, false);
        assert_eq!(e.caret(), "hé".len());
        e.set_caret(2, false); // lands inside 'é' (bytes 1..3): must floor
        assert!(e.text().is_char_boundary(e.caret()));
        e.end(false);
        e.move_left(false, true);
        assert_eq!(&e.text()[e.caret()..], "日本");
    }

    #[test]
    fn word_movement_and_selection_collapse() {
        let mut e = ed("a.b/c  d");
        e.home(false);
        e.move_right(false, true);
        assert_eq!(e.caret(), 2); // a. -> b
        e.move_right(true, true);
        assert_eq!(e.selection(), Some((2, 4)));
        e.move_left(false, false); // collapses to selection start, no extra step
        assert_eq!((e.caret(), e.selection()), (2, None));
        e.select_all();
        e.move_right(false, false);
        assert_eq!(e.caret(), e.text().len());
    }

    #[test]
    fn double_click_selects_word_or_run() {
        let mut e = ed("let url = x");
        e.select_word_at(5);
        assert_eq!(e.selected_text(), Some("url"));
        e.select_word_at(3);
        assert_eq!(e.selected_text(), Some(" "));
        e.select_word_at(100); // past end -> last word
        assert_eq!(e.selected_text(), Some("x"));
    }

    #[test]
    fn cut_copy_undo_redo() {
        let mut e = ed("abc def");
        e.set_caret(0, false);
        e.set_caret(3, true);
        assert_eq!(e.copy().as_deref(), Some("abc"));
        assert_eq!(e.cut().as_deref(), Some("abc"));
        assert_eq!(e.text(), " def");
        e.insert("X");
        assert_eq!(e.text(), "X def");
        assert!(e.undo());
        assert_eq!(e.text(), " def");
        assert!(e.undo());
        assert_eq!(e.text(), "abc def");
        assert!(e.redo());
        assert_eq!(e.text(), " def");
        assert!(!{ e.undo(); e.undo(); e.undo() }); // history exhausted
        e.insert("z");
        assert!(!e.redo()); // new edit clears redo
    }
}
