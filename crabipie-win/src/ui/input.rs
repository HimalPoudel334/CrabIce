//! Platform-neutral input vocabulary shared by the window layer and widgets.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Left,
    Right,
    Up,
    Down,
    Tab,
    Home,
    End,
    Backspace,
    Delete,
    Enter,
    Escape,
    /// Uppercase ASCII letter, for shortcuts (Ctrl+C etc).
    Letter(char),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub ctrl: bool,
    pub shift: bool,
}

pub trait Clipboard {
    fn get_text(&mut self) -> Option<String>;
    fn set_text(&mut self, s: &str);
}
