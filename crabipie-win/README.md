# crabipie-win (step 2)

Native Win32 + Direct2D/DirectWrite shell. No web components.

Run:   cargo run -p crabipie-win
Test:  cargo test -p crabipie-win     (UI logic; runs on any OS)

What works: window + DPI-aware Direct2D rendering, tab bar (select, close,
new, hover, middle-click close, Ctrl+T / Ctrl+W), tabs loaded read-only from
~/.crabipie. Nothing is written to disk yet, so it cannot disturb the iced
app's saved tabs.

Layout:
  src/ui/      geometry, theme, Canvas trait, tab bar   (no Windows types)
  src/app.rs   state + input handling                   (no Windows types)
  src/render/  Canvas implemented with Direct2D/DirectWrite   (Windows only)
  src/window.rs  HWND, WndProc, message loop                  (Windows only)

`windows` is pinned to =0.58.0: its APIs take raw pointers (Some(&x as *const _)).
Newer releases switched to references, so don't bump it without porting.
