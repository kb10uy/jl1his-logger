use std::ptr;

use windows_sys::{
    Win32::{
        Foundation::{HWND, LPARAM},
        System::Threading::GetCurrentProcessId,
        UI::{
            Shell::{NIF_INFO, NIIF_NONE, NIIF_RESPECT_QUIET_TIME, NIM_MODIFY, NOTIFYICONDATAW, Shell_NotifyIconW},
            WindowsAndMessaging::{EnumWindows, GetClassNameW, GetWindowThreadProcessId},
        },
    },
    core::BOOL,
};

const TRAY_CLASS: &str = "tray_icon_app";
const TRAY_ID: u32 = 2;

unsafe extern "system" fn find(hwnd: HWND, found: LPARAM) -> BOOL {
    let mut process = 0;
    unsafe { GetWindowThreadProcessId(hwnd, &mut process) };
    if process != unsafe { GetCurrentProcessId() } {
        return 1;
    }
    let mut class = [0u16; 32];
    let length = unsafe { GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32) };
    if String::from_utf16_lossy(&class[..length.max(0) as usize]) != TRAY_CLASS {
        return 1;
    }
    unsafe { *(found as *mut HWND) = hwnd };
    0
}

fn fill<const N: usize>(target: &mut [u16; N], text: &str) {
    let mut length = 0;
    for c in text.chars() {
        let mut buffer = [0u16; 2];
        let units = c.encode_utf16(&mut buffer);
        if length + units.len() >= N {
            break;
        }
        target[length..length + units.len()].copy_from_slice(units);
        length += units.len();
    }
}

pub fn show(title: &str, body: &str) -> bool {
    let mut hwnd: HWND = ptr::null_mut();
    unsafe { EnumWindows(Some(find), &mut hwnd as *mut HWND as LPARAM) };
    if hwnd.is_null() {
        return false;
    }
    let mut data = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_ID,
        uFlags: NIF_INFO,
        dwInfoFlags: NIIF_NONE | NIIF_RESPECT_QUIET_TIME,
        ..Default::default()
    };
    fill(&mut data.szInfoTitle, title);
    fill(&mut data.szInfo, body);
    unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) != 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cuts_text_to_fit_with_its_terminator() {
        let mut target = [0xffffu16; 4];
        fill(&mut target, "JA1ABC");
        assert_eq!(target, [b'J' as u16, b'A' as u16, b'1' as u16, 0xffff]);
    }

    #[test]
    fn never_splits_a_surrogate_pair() {
        let mut target = [0u16; 4];
        fill(&mut target, "a📻b");
        assert_eq!(String::from_utf16_lossy(&target), "a📻\0");
        let mut short = [0u16; 3];
        fill(&mut short, "a📻");
        assert_eq!(short, [b'a' as u16, 0, 0]);
    }
}
