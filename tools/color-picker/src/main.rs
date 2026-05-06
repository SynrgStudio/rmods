#![windows_subsystem = "windows"]

use std::env;
use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::null_mut;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{GetDC, GetPixel, ReleaseDC};
use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_ESCAPE, VK_LBUTTON};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetCursorPos, LoadCursorW,
    MessageBoxW, PeekMessageW, RegisterClassW, SetCursor, ShowWindow, CS_HREDRAW, CS_VREDRAW,
    IDC_CROSS, MB_ICONERROR, MB_OK, MSG, PM_REMOVE, SW_HIDE, WNDCLASSW, WS_EX_TOOLWINDOW,
    WS_POPUP,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Hex,
    Rgb,
    Hsl,
    All,
}

fn main() {
    let format = parse_format();
    match pick_color(format) {
        Ok(Some(value)) => {
            if let Err(error) = copy_to_clipboard(&value) {
                show_error(&format!("Failed to copy color: {error}"));
                std::process::exit(1);
            }
            println!("{value}");
        }
        Ok(None) => std::process::exit(1),
        Err(error) => {
            show_error(&error);
            std::process::exit(1);
        }
    }
}

fn parse_format() -> Format {
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        let value = if arg == "--format" || arg == "-f" {
            args.next().unwrap_or_default()
        } else if let Some(value) = arg.strip_prefix("--format=") {
            value.to_string()
        } else {
            continue;
        };
        return match value.trim().to_ascii_lowercase().as_str() {
            "rgb" => Format::Rgb,
            "hsl" => Format::Hsl,
            "all" => Format::All,
            _ => Format::Hex,
        };
    }
    Format::Hex
}

fn pick_color(format: Format) -> Result<Option<String>, String> {
    let window = create_hidden_window()?;
    unsafe {
        let cursor = LoadCursorW(None, IDC_CROSS).map_err(|error| error.to_string())?;
        SetCursor(cursor);
    }

    let mut was_left_down = unsafe { key_down(i32::from(VK_LBUTTON.0)) };
    loop {
        pump_messages();
        unsafe {
            let cursor = LoadCursorW(None, IDC_CROSS).map_err(|error| error.to_string())?;
            SetCursor(cursor);
        }

        if unsafe { key_down(i32::from(VK_ESCAPE.0)) } {
            unsafe { let _ = DestroyWindow(window); };
            return Ok(None);
        }

        let left_down = unsafe { key_down(i32::from(VK_LBUTTON.0)) };
        if left_down && !was_left_down {
            let mut point = POINT::default();
            unsafe { GetCursorPos(&mut point).map_err(|error| error.to_string())? };
            let color = sample_screen_pixel(point.x, point.y)?;
            unsafe { let _ = DestroyWindow(window); };
            return Ok(Some(format_color(color, format)));
        }
        was_left_down = left_down;
        std::thread::sleep(std::time::Duration::from_millis(8));
    }
}

unsafe fn key_down(vk: i32) -> bool {
    (GetAsyncKeyState(vk) as u16 & 0x8000) != 0
}

fn sample_screen_pixel(x: i32, y: i32) -> Result<(u8, u8, u8), String> {
    unsafe {
        let dc = GetDC(None);
        if dc.is_invalid() {
            return Err("GetDC failed".to_string());
        }
        let raw = GetPixel(dc, x, y);
        let _ = ReleaseDC(None, dc);
        if raw == COLORREF(0xFFFF_FFFF) {
            return Err("GetPixel failed".to_string());
        }
        let value = raw.0;
        let r = (value & 0xFF) as u8;
        let g = ((value >> 8) & 0xFF) as u8;
        let b = ((value >> 16) & 0xFF) as u8;
        Ok((r, g, b))
    }
}

fn format_color((r, g, b): (u8, u8, u8), format: Format) -> String {
    let hex = format!("#{r:02X}{g:02X}{b:02X}");
    match format {
        Format::Hex => hex,
        Format::Rgb => format!("rgb({r}, {g}, {b})"),
        Format::Hsl => {
            let (h, s, l) = rgb_to_hsl(r, g, b);
            format!("hsl({h:.0}, {s:.0}%, {l:.0}%)")
        }
        Format::All => {
            let (h, s, l) = rgb_to_hsl(r, g, b);
            format!("{hex}\nrgb({r}, {g}, {b})\nhsl({h:.0}, {s:.0}%, {l:.0}%)")
        }
    }
}

fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f64, f64, f64) {
    let r = f64::from(r) / 255.0;
    let g = f64::from(g) / 255.0;
    let b = f64::from(b) / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if (max - min).abs() < f64::EPSILON {
        return (0.0, 0.0, l * 100.0);
    }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if (max - r).abs() < f64::EPSILON {
        ((g - b) / d + if g < b { 6.0 } else { 0.0 }) / 6.0
    } else if (max - g).abs() < f64::EPSILON {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    (h * 360.0, s * 100.0, l * 100.0)
}

fn copy_to_clipboard(value: &str) -> Result<(), String> {
    let mut wide = value.encode_utf16().collect::<Vec<_>>();
    wide.push(0);
    let bytes = wide.len() * size_of::<u16>();
    unsafe {
        let handle = GlobalAlloc(GMEM_MOVEABLE, bytes).map_err(|error| error.to_string())?;
        let ptr = GlobalLock(handle) as *mut u16;
        if ptr.is_null() {
            return Err("GlobalLock failed".to_string());
        }
        ptr.copy_from_nonoverlapping(wide.as_ptr(), wide.len());
        let _ = GlobalUnlock(handle);
        OpenClipboard(None).map_err(|error| error.to_string())?;
        EmptyClipboard().map_err(|error| {
            let _ = CloseClipboard();
            error.to_string()
        })?;
        SetClipboardData(13, HANDLE(handle.0)).map_err(|error| {
            let _ = CloseClipboard();
            error.to_string()
        })?;
        CloseClipboard().map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn create_hidden_window() -> Result<HWND, String> {
    unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    unsafe {
        let instance = HINSTANCE::default();
        let class_name = w!("RpackColorPickerHiddenWindow");
        let class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            hInstance: instance,
            lpszClassName: class_name,
            ..Default::default()
        };
        RegisterClassW(&class);
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class_name,
            w!("Color Picker"),
            WS_POPUP,
            0,
            0,
            1,
            1,
            None,
            None,
            instance,
            Some(null_mut::<c_void>()),
        )
        .map_err(|error| error.to_string())?;
        let _ = ShowWindow(hwnd, SW_HIDE);
        Ok(hwnd)
    }
}

fn pump_messages() {
    unsafe {
        let mut msg = MSG::default();
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            let _ = DispatchMessageW(&msg);
        }
    }
}

fn show_error(message: &str) {
    let wide = message.encode_utf16().chain(std::iter::once(0)).collect::<Vec<_>>();
    unsafe {
        let _ = MessageBoxW(None, PCWSTR(wide.as_ptr()), w!("Color Picker"), MB_OK | MB_ICONERROR);
    }
}
