//! Win32 window: class registration, message loop, WndProc. Translates raw
//! messages into DIP-space calls on `App` and paints through `Renderer`.

use std::ffi::c_void;

use windows::core::{w, Error, Result};
use windows::Win32::Foundation::{BOOL, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{InvalidateRect, ValidateRect, HBRUSH};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    GetDpiForWindow, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT,
    VK_BACK, VK_CONTROL, VK_DELETE, VK_DOWN, VK_END, VK_TAB, VK_UP, VK_ESCAPE, VK_HOME, VK_LEFT, VK_RETURN, VK_RIGHT,
    VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::app::App;
use crate::platform::clipboard::WinClipboard;
use crate::platform::file_dialog::pick_file;
use crate::render::Renderer;
use crate::ui::input::{Key, Mods};
use crabipie_core::{run_request, AuthRefreshOutcome, FlowEvent, ResponseEvent};

// Not in the bindings I target (WM_MOUSELEAVE lives behind the Controls feature).
const WM_MOUSELEAVE: u32 = 0x02A3;
/// Posted by the async runtime; lparam owns a `Box<BridgeMsg>`.
const WM_APP_RESPONSE: u32 = 0x8001; // WM_APP + 1
const BLINK_TIMER: usize = 1;
const BLINK_MS: u32 = 530;

enum BridgeMsg {
    Net { tab_id: usize, ev: ResponseEvent },
    Token { env_id: usize, res: std::result::Result<AuthRefreshOutcome, String> },
}

/// Called from tokio worker threads: hands an engine event to the UI thread.
fn post_to_ui(hwnd_raw: isize, msg: BridgeMsg) {
    let ptr = Box::into_raw(Box::new(msg));
    let hwnd = HWND(hwnd_raw as *mut c_void);
    if unsafe { PostMessageW(hwnd, WM_APP_RESPONSE, WPARAM(0), LPARAM(ptr as isize)) }.is_err() {
        // window already gone: reclaim the message
        drop(unsafe { Box::from_raw(ptr) });
    }
}

struct Window {
    hwnd: HWND,
    renderer: Renderer,
    app: App,
    tracking_mouse: bool,
    /// First half of a UTF-16 surrogate pair delivered by WM_CHAR.
    high_surrogate: Option<u16>,
    rt: tokio::runtime::Runtime,
}

impl Window {
    fn scale(&self) -> f32 {
        let dpi = unsafe { GetDpiForWindow(self.hwnd) };
        if dpi == 0 { 1.0 } else { dpi as f32 / 96.0 }
    }

    fn invalidate(&self) {
        unsafe {
            let _ = InvalidateRect(self.hwnd, None, BOOL::from(false));
        }
    }

    fn paint(&mut self) {
        if !self.renderer.begin_draw() {
            return;
        }
        let (w, h) = self.renderer.size();
        self.app.on_resize(w, h);
        self.app.paint(&mut self.renderer);
        self.renderer.end_draw();
    }

    /// Starts any request the app queued (Send button / Enter).
    fn dispatch_sends(&mut self) {
        while let Some(job) = self.app.take_send() {
            let hwnd_raw = self.hwnd.0 as isize;
            let tab_id = job.tab_id;
            let env_id = job.env.as_ref().map(|e| e.id);
            self.rt.spawn(async move {
                run_request(job.state, job.env, job.jar, job.cancel, move |fe| match fe {
                    FlowEvent::Net(ev) => post_to_ui(hwnd_raw, BridgeMsg::Net { tab_id, ev }),
                    FlowEvent::Token(res) => {
                        if let Some(env_id) = env_id {
                            post_to_ui(hwnd_raw, BridgeMsg::Token { env_id, res })
                        }
                    }
                })
                .await;
            });
        }
    }

    /// Mouse position from an lparam, converted to DIPs.
    fn pos(&self, lparam: LPARAM) -> (f32, f32) {
        let s = self.scale();
        (get_x(lparam) / s, get_y(lparam) / s)
    }
}

fn get_x(lparam: LPARAM) -> f32 {
    (lparam.0 & 0xFFFF) as i16 as f32
}
fn get_y(lparam: LPARAM) -> f32 {
    ((lparam.0 >> 16) & 0xFFFF) as i16 as f32
}

fn key_down(vk: i32) -> bool {
    unsafe { GetKeyState(vk) < 0 }
}

fn map_key(vk: u16) -> Option<Key> {
    match vk {
        v if v == VK_LEFT.0 => Some(Key::Left),
        v if v == VK_RIGHT.0 => Some(Key::Right),
        v if v == VK_UP.0 => Some(Key::Up),
        v if v == VK_DOWN.0 => Some(Key::Down),
        v if v == VK_TAB.0 => Some(Key::Tab),
        v if v == VK_HOME.0 => Some(Key::Home),
        v if v == VK_END.0 => Some(Key::End),
        v if v == VK_BACK.0 => Some(Key::Backspace),
        v if v == VK_DELETE.0 => Some(Key::Delete),
        v if v == VK_RETURN.0 => Some(Key::Enter),
        v if v == VK_ESCAPE.0 => Some(Key::Escape),
        0x41..=0x5A => Some(Key::Letter(vk as u8 as char)),
        _ => None,
    }
}

pub fn run() -> Result<()> {
    unsafe {
        // Must happen before any window exists.
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        let hinstance: HINSTANCE = GetModuleHandleW(None)?.into();
        let class_name = w!("CrabiPieWindow");

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW | CS_DBLCLKS,
            lpfnWndProc: Some(wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: HICON::default(),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            hbrBackground: HBRUSH::default(),
            lpszMenuName: windows::core::PCWSTR::null(),
            lpszClassName: class_name,
            hIconSm: HICON::default(),
        };
        if RegisterClassExW(&wc) == 0 {
            return Err(Error::from_win32());
        }

        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");
        let app = App::from_disk(&rt);

        let window = Box::new(Window {
            hwnd: HWND::default(),
            renderer: Renderer::new()?,
            app,
            tracking_mouse: false,
            high_surrogate: None,
            rt,
        });
        let ptr = Box::into_raw(window);

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class_name,
            w!("CrabiPie"),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            1500,
            800,
            None,
            None,
            hinstance,
            Some(ptr as *const c_void),
        )?;

        // caret blink
        SetTimer(hwnd, BLINK_TIMER, BLINK_MS, None);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg); // generates WM_CHAR from WM_KEYDOWN
            DispatchMessageW(&msg);
        }
        Ok(())
    }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        if msg == WM_NCCREATE {
            let cs = lparam.0 as *const CREATESTRUCTW;
            let ptr = (*cs).lpCreateParams as *mut Window;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, ptr as isize);
            if let Some(win) = ptr.as_mut() {
                win.hwnd = hwnd;
                win.renderer.attach(hwnd);
            }
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }

        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Window;
        let Some(win) = ptr.as_mut() else {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        };

        match msg {
            WM_PAINT => {
                win.paint();
                let _ = ValidateRect(hwnd, None);
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1), // Direct2D paints everything
            WM_SIZE => {
                let w = (lparam.0 & 0xFFFF) as u32;
                let h = ((lparam.0 >> 16) & 0xFFFF) as u32;
                win.renderer.resize(w, h);
                win.invalidate();
                LRESULT(0)
            }
            WM_DPICHANGED => {
                let r = &*(lparam.0 as *const RECT);
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                // SetWindowPos re-enters this WndProc (WM_SIZE), so re-borrow.
                let win = &mut *ptr;
                win.renderer.update_dpi();
                win.invalidate();
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                if !win.tracking_mouse {
                    let mut t = TRACKMOUSEEVENT {
                        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    let _ = TrackMouseEvent(&mut t);
                    win.tracking_mouse = true;
                }
                let (x, y) = win.pos(lparam);
                if win.app.on_mouse_move(&mut win.renderer, x, y) {
                    win.invalidate();
                }
                LRESULT(0)
            }
            WM_MOUSELEAVE => {
                win.tracking_mouse = false;
                if win.app.on_mouse_leave() {
                    win.invalidate();
                }
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                // Capture so drag-selecting keeps working outside the window.
                SetCapture(hwnd);
                let (x, y) = win.pos(lparam);
                let shift = key_down(VK_SHIFT.0 as i32);
                let mut cb = WinClipboard { hwnd };
                if win.app.on_left_down(&mut win.renderer, &mut cb, x, y, shift) {
                    win.invalidate();
                }
                win.dispatch_sends();
                if let Some(row) = win.app.take_file_request() {
                    if let Some(path) = pick_file(hwnd) {
                        win.app.set_form_file(row, path);
                        win.invalidate();
                    }
                }
                LRESULT(0)
            }
            WM_LBUTTONDBLCLK => {
                SetCapture(hwnd);
                let (x, y) = win.pos(lparam);
                if win.app.on_double_click(&mut win.renderer, x, y) {
                    win.invalidate();
                }
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                let _ = ReleaseCapture();
                win.app.on_left_up();
                LRESULT(0)
            }
            WM_MBUTTONUP => {
                let (x, y) = win.pos(lparam);
                if win.app.on_middle_click(x, y) {
                    win.invalidate();
                }
                LRESULT(0)
            }
            WM_SETCURSOR => {
                if (lparam.0 & 0xFFFF) as u32 == HTCLIENT && win.app.text_cursor_active() {
                    if let Ok(c) = LoadCursorW(None, IDC_IBEAM) {
                        let _ = SetCursor(c);
                        return LRESULT(1);
                    }
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_KEYDOWN => {
                if let Some(key) = map_key(wparam.0 as u16) {
                    let mods = Mods {
                        ctrl: key_down(VK_CONTROL.0 as i32),
                        shift: key_down(VK_SHIFT.0 as i32),
                    };
                    let mut cb = WinClipboard { hwnd };
                    if win.app.on_key(key, mods, &mut cb) {
                        win.invalidate();
                        win.dispatch_sends();
                        return LRESULT(0);
                    }
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_CHAR => {
                let unit = wparam.0 as u16;
                let ch = if (0xD800..0xDC00).contains(&unit) {
                    win.high_surrogate = Some(unit); // wait for the low half
                    return LRESULT(0);
                } else if (0xDC00..0xE000).contains(&unit) {
                    win.high_surrogate
                        .take()
                        .and_then(|hi| char::decode_utf16([hi, unit]).next())
                        .and_then(|r| r.ok())
                } else {
                    char::from_u32(unit as u32)
                };
                if let Some(ch) = ch {
                    if win.app.on_char(ch) {
                        win.invalidate();
                    }
                }
                LRESULT(0)
            }
            WM_APP_RESPONSE => {
                let repaint = match *Box::from_raw(lparam.0 as *mut BridgeMsg) {
                    BridgeMsg::Net { tab_id, ev } => win.app.on_response(tab_id, ev),
                    BridgeMsg::Token { env_id, res } => win.app.on_token(env_id, res),
                };
                if repaint {
                    win.invalidate();
                }
                LRESULT(0)
            }
            WM_MOUSEWHEEL => {
                let notches = ((wparam.0 >> 16) & 0xFFFF) as u16 as i16 as f32 / 120.0;
                if win.app.on_wheel(notches) {
                    win.invalidate();
                }
                LRESULT(0)
            }
            WM_TIMER => {
                if wparam.0 == BLINK_TIMER && win.app.blink() {
                    win.invalidate();
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                let _ = KillTimer(hwnd, BLINK_TIMER);
                PostQuitMessage(0);
                LRESULT(0)
            }
            WM_NCDESTROY => {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                drop(Box::from_raw(ptr));
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}
