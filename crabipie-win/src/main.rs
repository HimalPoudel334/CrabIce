#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

// Platform-independent: geometry, theme, widgets, app logic. Draws through
// the `Canvas` trait, so it is unit-testable without a window.
mod app;
mod ui;

// Windows-only: the thin layer that owns the HWND and Direct2D/DirectWrite.
#[cfg(windows)]
mod render;
#[cfg(windows)]
mod window;

#[cfg(windows)]
fn main() -> windows::core::Result<()> {
    window::run()
}

#[cfg(not(windows))]
fn main() {
    eprintln!("crabipie-win only runs on Windows (use `cargo test` to run the UI-logic tests).");
}
