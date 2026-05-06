#![windows_subsystem = "windows"]

use std::ptr::null_mut;

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    keybd_event, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_VOLUME_DOWN, VK_VOLUME_MUTE,
    VK_VOLUME_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetClassNameW, GetMessageW, GetParent, SetWindowsHookExW,
    UnhookWindowsHookEx, WindowFromPoint, HHOOK, MSG, MSLLHOOKSTRUCT, WH_MOUSE_LL, WM_MBUTTONDOWN,
    WM_MOUSEWHEEL,
};

const TASKBAR_CLASSES: [&str; 2] = ["Shell_TrayWnd", "Shell_SecondaryTrayWnd"];

fn main() {
    let hook =
        match unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), HINSTANCE(null_mut()), 0) }
        {
            Ok(hook) => hook,
            Err(_) => return,
        };
    if hook.is_invalid() {
        return;
    }

    let mut msg = MSG::default();
    while unsafe { GetMessageW(&mut msg, HWND(null_mut()), 0, 0) }.0 > 0 {
        unsafe {
            DispatchMessageW(&msg);
        }
    }

    unsafe {
        let _ = UnhookWindowsHookEx(hook);
    }
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code < 0 {
        return unsafe { CallNextHookEx(HHOOK(null_mut()), code, wparam, lparam) };
    }

    let event = wparam.0 as u32;
    if event != WM_MOUSEWHEEL && event != WM_MBUTTONDOWN {
        return unsafe { CallNextHookEx(HHOOK(null_mut()), code, wparam, lparam) };
    }

    let mouse = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
    if !is_over_taskbar(mouse.pt) {
        return unsafe { CallNextHookEx(HHOOK(null_mut()), code, wparam, lparam) };
    }

    match event {
        WM_MOUSEWHEEL => {
            let delta = high_word(mouse.mouseData) as i16;
            if delta > 0 {
                tap_key(VK_VOLUME_UP);
            } else if delta < 0 {
                tap_key(VK_VOLUME_DOWN);
            }
        }
        WM_MBUTTONDOWN => {
            if !is_over_taskbar_background(mouse.pt) {
                return unsafe { CallNextHookEx(HHOOK(null_mut()), code, wparam, lparam) };
            }
            tap_key(VK_VOLUME_MUTE);
        }
        _ => {}
    }

    LRESULT(1)
}

fn is_over_taskbar_background(point: POINT) -> bool {
    let hwnd = unsafe { WindowFromPoint(point) };
    class_name(hwnd)
        .map(|class_name| TASKBAR_CLASSES.iter().any(|class| class_name == *class))
        .unwrap_or(false)
}

fn is_over_taskbar(point: POINT) -> bool {
    let mut hwnd = unsafe { WindowFromPoint(point) };
    while !hwnd.0.is_null() {
        if let Some(class_name) = class_name(hwnd) {
            if TASKBAR_CLASSES.iter().any(|class| class_name == *class) {
                return true;
            }
        }
        hwnd = match unsafe { GetParent(hwnd) } {
            Ok(parent) => parent,
            Err(_) => HWND(null_mut()),
        };
    }
    false
}

fn class_name(hwnd: HWND) -> Option<String> {
    let mut buffer = [0u16; 256];
    let len = unsafe { GetClassNameW(hwnd, &mut buffer) };
    if len <= 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buffer[..len as usize]))
}

fn tap_key(key: VIRTUAL_KEY) {
    unsafe {
        keybd_event(key.0 as u8, 0, KEYBD_EVENT_FLAGS(0), 0);
        keybd_event(key.0 as u8, 0, KEYEVENTF_KEYUP, 0);
    }
}

fn high_word(value: u32) -> u16 {
    ((value >> 16) & 0xffff) as u16
}
