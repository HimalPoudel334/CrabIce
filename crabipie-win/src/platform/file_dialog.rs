//! Native "Open file" dialog (comdlg32), used by form-data file fields.

use windows::core::PWSTR;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};

pub fn pick_file(owner: HWND) -> Option<String> {
    unsafe {
        let mut buf = [0u16; 1024];
        let mut ofn: OPENFILENAMEW = std::mem::zeroed();
        ofn.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
        ofn.hwndOwner = owner;
        ofn.lpstrFile = PWSTR(buf.as_mut_ptr());
        ofn.nMaxFile = buf.len() as u32;
        ofn.Flags = OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_EXPLORER;
        if GetOpenFileNameW(&mut ofn).as_bool() {
            let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            Some(String::from_utf16_lossy(&buf[..len]))
        } else {
            None
        }
    }
}
