//! Windows clipboard (CF_UNICODETEXT) behind the toolkit-free `Clipboard` trait.

use windows::Win32::Foundation::{HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE};

use crate::ui::input::Clipboard;

const CF_UNICODETEXT: u32 = 13;

pub struct WinClipboard {
    pub hwnd: HWND,
}

impl Clipboard for WinClipboard {
    fn get_text(&mut self) -> Option<String> {
        unsafe {
            OpenClipboard(self.hwnd).ok()?;
            let text = (|| {
                let handle = GetClipboardData(CF_UNICODETEXT).ok()?;
                let hglobal = HGLOBAL(handle.0);
                let ptr = GlobalLock(hglobal) as *const u16;
                if ptr.is_null() {
                    return None;
                }
                // The block is NUL-terminated; never read past its allocation.
                let max = GlobalSize(hglobal) / 2;
                let mut len = 0;
                while len < max && *ptr.add(len) != 0 {
                    len += 1;
                }
                let s = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len));
                let _ = GlobalUnlock(hglobal);
                Some(s)
            })();
            let _ = CloseClipboard();
            text
        }
    }

    fn set_text(&mut self, s: &str) {
        unsafe {
            if OpenClipboard(self.hwnd).is_err() {
                return;
            }
            let _ = EmptyClipboard();
            let wide: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
            if let Ok(hglobal) = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2) {
                let ptr = GlobalLock(hglobal) as *mut u16;
                if !ptr.is_null() {
                    std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr, wide.len());
                    let _ = GlobalUnlock(hglobal);
                    // On success the clipboard owns the memory.
                    let _ = SetClipboardData(CF_UNICODETEXT, HANDLE(hglobal.0));
                }
            }
            let _ = CloseClipboard();
        }
    }
}
