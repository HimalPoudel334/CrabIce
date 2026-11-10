//! Local wall-clock time for log timestamps.

use windows::Win32::System::SystemInformation::GetLocalTime;

/// (hour, minute, second, millisecond) in the user's local time zone.
pub fn local_clock() -> (u8, u8, u8, u16) {
    let t = unsafe { GetLocalTime() };
    (t.wHour as u8, t.wMinute as u8, t.wSecond as u8, t.wMilliseconds)
}
