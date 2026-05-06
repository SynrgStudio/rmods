#![windows_subsystem = "windows"]

use std::path::Path;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::core::PWSTR;
use windows::Win32::Foundation::{
    CloseHandle, BOOL, HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM,
};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    keybd_event, GetAsyncKeyState, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_CONTROL,
    VK_MENU, VK_SHIFT, VK_T, VK_TAB, VK_W,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetForegroundWindow, GetMessageW, GetWindowThreadProcessId,
    SetWindowsHookExW, UnhookWindowsHookEx, HHOOK, MSG, MSLLHOOKSTRUCT, WH_MOUSE_LL,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_RBUTTONUP,
};

const TARGET_EXE: &str = "thorium.exe";

static SUPPRESS_LEFT_UP: AtomicBool = AtomicBool::new(false);
static SUPPRESS_RIGHT_UP: AtomicBool = AtomicBool::new(false);

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
    if event == WM_LBUTTONUP && SUPPRESS_LEFT_UP.swap(false, Ordering::SeqCst) {
        return LRESULT(1);
    }
    if event == WM_RBUTTONUP && SUPPRESS_RIGHT_UP.swap(false, Ordering::SeqCst) {
        return LRESULT(1);
    }

    if event != WM_MOUSEWHEEL
        && event != WM_LBUTTONDOWN
        && event != WM_LBUTTONUP
        && event != WM_RBUTTONDOWN
        && event != WM_RBUTTONUP
    {
        return unsafe { CallNextHookEx(HHOOK(null_mut()), code, wparam, lparam) };
    }
    if !alt_down() || !foreground_is_target() {
        return unsafe { CallNextHookEx(HHOOK(null_mut()), code, wparam, lparam) };
    }

    let mouse = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
    match event {
        WM_MOUSEWHEEL => {
            let delta = high_word(mouse.mouseData) as i16;
            if delta > 0 {
                send_combo_without_alt(&[VK_CONTROL], VK_TAB);
            } else if delta < 0 {
                send_combo_without_alt(&[VK_CONTROL, VK_SHIFT], VK_TAB);
            }
        }
        WM_LBUTTONDOWN => {
            SUPPRESS_LEFT_UP.store(true, Ordering::SeqCst);
            send_combo_without_alt(&[VK_CONTROL], VK_W);
        }
        WM_RBUTTONDOWN => {
            SUPPRESS_RIGHT_UP.store(true, Ordering::SeqCst);
            send_combo_without_alt(&[VK_CONTROL, VK_SHIFT], VK_T);
        }
        WM_LBUTTONUP | WM_RBUTTONUP => {}
        _ => {}
    }

    LRESULT(1)
}

fn foreground_is_target() -> bool {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0.is_null() {
        return false;
    }

    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }
    if pid == 0 {
        return false;
    }

    let process = match unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, BOOL(0), pid) } {
        Ok(handle) => handle,
        Err(_) => return false,
    };
    let result = process_exe_name(process)
        .map(|name| name.eq_ignore_ascii_case(TARGET_EXE))
        .unwrap_or(false);
    unsafe {
        let _ = CloseHandle(process);
    }
    result
}

fn process_exe_name(process: HANDLE) -> Option<String> {
    let mut buffer = [0u16; 32768];
    let mut len = buffer.len() as u32;
    unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut len,
        )
        .ok()?;
    }
    let path = String::from_utf16_lossy(&buffer[..len as usize]);
    Path::new(&path)
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
}

fn alt_down() -> bool {
    unsafe { (GetAsyncKeyState(i32::from(VK_MENU.0)) as u16 & 0x8000) != 0 }
}

fn send_combo_without_alt(modifiers: &[VIRTUAL_KEY], key: VIRTUAL_KEY) {
    key_up(VK_MENU);
    send_combo(modifiers, key);
    key_down(VK_MENU);
}

fn send_combo(modifiers: &[VIRTUAL_KEY], key: VIRTUAL_KEY) {
    for modifier in modifiers {
        key_down(*modifier);
    }
    tap_key(key);
    for modifier in modifiers.iter().rev() {
        key_up(*modifier);
    }
}

fn tap_key(key: VIRTUAL_KEY) {
    key_down(key);
    key_up(key);
}

fn key_down(key: VIRTUAL_KEY) {
    unsafe {
        keybd_event(key.0 as u8, 0, KEYBD_EVENT_FLAGS(0), 0);
    }
}

fn key_up(key: VIRTUAL_KEY) {
    unsafe {
        keybd_event(key.0 as u8, 0, KEYEVENTF_KEYUP, 0);
    }
}

fn high_word(value: u32) -> u16 {
    ((value >> 16) & 0xffff) as u16
}
