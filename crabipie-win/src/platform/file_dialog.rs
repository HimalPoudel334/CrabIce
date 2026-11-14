//! Native "Open file" dialog (comdlg32), used by form-data file fields.

use windows::core::{w, PCWSTR, PWSTR};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, GetSaveFileNameW, OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST, OPENFILENAMEW,
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

/// Native "Save as" dialog for a .json file, pre-filled with `default_name`.
pub fn save_file(owner: HWND, default_name: &str) -> Option<String> {
    unsafe {
        let mut buf = [0u16; 1024];
        for (i, u) in default_name.encode_utf16().take(buf.len() - 1).enumerate() {
            buf[i] = u;
        }
        let filter: Vec<u16> = "JSON (*.json)\0*.json\0All files\0*.*\0\0".encode_utf16().collect();
        let mut ofn: OPENFILENAMEW = std::mem::zeroed();
        ofn.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
        ofn.hwndOwner = owner;
        ofn.lpstrFilter = PCWSTR(filter.as_ptr());
        ofn.lpstrDefExt = w!("json");
        ofn.lpstrFile = PWSTR(buf.as_mut_ptr());
        ofn.nMaxFile = buf.len() as u32;
        ofn.Flags = OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST | OFN_EXPLORER;
        if GetSaveFileNameW(&mut ofn).as_bool() {
            let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            Some(String::from_utf16_lossy(&buf[..len]))
        } else {
            None
        }
    }
}
