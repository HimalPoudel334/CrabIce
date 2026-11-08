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
    GetKeyState, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT, VK_CONTROL,
};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::app::App;
use crate::render::Renderer;

// Not in the bindings I target (WM_MOUSELEAVE lives behind the Controls feature).
const WM_MOUSELEAVE: u32 = 0x02A3;

struct Window {
    hwnd: HWND,
    renderer: Renderer,
    app: App,
    tracking_mouse: bool,
    // Kept alive for the window's lifetime; the async bridge (step 4) uses it.
    _rt: tokio::runtime::Runtime,
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
}

fn get_x(lparam: LPARAM) -> f32 {
    (lparam.0 & 0xFFFF) as i16 as f32
}
fn get_y(lparam: LPARAM) -> f32 {
    ((lparam.0 >> 16) & 0xFFFF) as i16 as f32
}

pub fn run() -> Result<()> {
    unsafe {
        // Must happen before any window exists.
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        let hinstance: HINSTANCE = GetModuleHandleW(None)?.into();
        let class_name = w!("CrabiPieWindow");

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
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
            _rt: rt,
        });
        let ptr = Box::into_raw(window);

        CreateWindowExW(
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

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
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
                let s = win.scale();
                if win.app.on_mouse_move(get_x(lparam) / s, get_y(lparam) / s) {
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
                let s = win.scale();
                if win.app.on_left_click(get_x(lparam) / s, get_y(lparam) / s) {
                    win.invalidate();
                }
                LRESULT(0)
            }
            WM_MBUTTONUP => {
                let s = win.scale();
                if win.app.on_middle_click(get_x(lparam) / s, get_y(lparam) / s) {
                    win.invalidate();
                }
                LRESULT(0)
            }
            WM_KEYDOWN => {
                let ctrl = GetKeyState(VK_CONTROL.0 as i32) < 0;
                if ctrl {
                    if let Some(ch) = char::from_u32(wparam.0 as u32) {
                        if win.app.on_ctrl_key(ch) {
                            win.invalidate();
                            return LRESULT(0);
                        }
                    }
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_DESTROY => {
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
