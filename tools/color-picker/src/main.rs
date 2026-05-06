#![windows_subsystem = "windows"]

use std::env;
use std::ffi::c_void;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateSolidBrush, DeleteDC, DeleteObject, FillRect, GetDC, GetPixel, ReleaseDC, SelectObject, SetBkColor, SetStretchBltMode, SetTextColor, StretchBlt, TextOutW, HBITMAP, HDC, HGDIOBJ, SRCCOPY, STRETCH_BLT_MODE};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_ESCAPE};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetCursorPos, GetSystemMetrics,
    GetMessageW, LoadCursorW, MessageBoxW, MoveWindow, PeekMessageW, PostThreadMessageW, RegisterClassW, SetCursor,
    SetWindowsHookExW, ShowWindow, UnhookWindowsHookEx, CS_HREDRAW, CS_VREDRAW, HHOOK, IDC_CROSS, MB_ICONERROR,
    MB_OK, MSG, PM_REMOVE, SM_CXSCREEN, SM_CYSCREEN, SW_SHOWNOACTIVATE, WH_MOUSE_LL, WM_LBUTTONDOWN, WM_QUIT,
    WNDCLASSW, WS_BORDER, WS_EX_TOPMOST, WS_EX_TOOLWINDOW, WS_POPUP,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Hex,
    Rgb,
    Hsl,
    All,
}

static LEFT_CLICKED: AtomicBool = AtomicBool::new(false);

const WIN_W: i32 = 230;
const WIN_H: i32 = 170;

struct MouseHookThread {
    thread_id: u32,
    handle: Option<JoinHandle<()>>,
}

struct DoubleBuffer {
    mem_dc: HDC,
    bitmap: HBITMAP,
    old_bitmap: HGDIOBJ,
}

impl DoubleBuffer {
    fn new(window_dc: HDC) -> Result<Self, String> {
        unsafe {
            let mem_dc = CreateCompatibleDC(window_dc);
            if mem_dc.is_invalid() {
                return Err("CreateCompatibleDC failed".to_string());
            }
            let bitmap = CreateCompatibleBitmap(window_dc, WIN_W, WIN_H);
            if bitmap.is_invalid() {
                let _ = DeleteDC(mem_dc);
                return Err("CreateCompatibleBitmap failed".to_string());
            }
            let old_bitmap = SelectObject(mem_dc, bitmap);
            Ok(Self { mem_dc, bitmap, old_bitmap })
        }
    }
}

impl Drop for DoubleBuffer {
    fn drop(&mut self) {
        unsafe {
            let _ = SelectObject(self.mem_dc, self.old_bitmap);
            let _ = DeleteObject(self.bitmap);
            let _ = DeleteDC(self.mem_dc);
        }
    }
}

impl Drop for MouseHookThread {
    fn drop(&mut self) {
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
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
    let window = create_picker_window()?;
    let screen_dc = unsafe { GetDC(None) };
    if screen_dc.is_invalid() {
        return Err("GetDC failed".to_string());
    }
    let window_dc = unsafe { GetDC(window) };
    if window_dc.is_invalid() {
        unsafe {
            let _ = ReleaseDC(None, screen_dc);
            let _ = DestroyWindow(window);
        }
        return Err("GetDC(window) failed".to_string());
    }
    let mouse_hook = start_mouse_hook_thread().map_err(|error| {
        unsafe {
            let _ = ReleaseDC(window, window_dc);
            let _ = ReleaseDC(None, screen_dc);
            let _ = DestroyWindow(window);
        }
        error
    })?;

    let painter = match DoubleBuffer::new(window_dc) {
        Ok(painter) => painter,
        Err(error) => {
            cleanup_picker(window, window_dc, screen_dc);
            drop(mouse_hook);
            return Err(error);
        }
    };

    unsafe {
        SetStretchBltMode(window_dc, STRETCH_BLT_MODE(1));
        SetStretchBltMode(painter.mem_dc, STRETCH_BLT_MODE(1));
        let cursor = LoadCursorW(None, IDC_CROSS).map_err(|error| error.to_string())?;
        SetCursor(cursor);
    }

    loop {
        pump_messages();
        unsafe {
            let cursor = LoadCursorW(None, IDC_CROSS).map_err(|error| error.to_string())?;
            SetCursor(cursor);
        }

        let mut point = POINT::default();
        unsafe { GetCursorPos(&mut point).map_err(|error| error.to_string())? };
        let color = sample_screen_pixel(screen_dc, point.x, point.y)?;
        update_picker_window(window, window_dc, &painter, screen_dc, point.x, point.y, color, format);

        if unsafe { key_down(i32::from(VK_ESCAPE.0)) } {
            cleanup_picker(window, window_dc, screen_dc);
            drop(mouse_hook);
            return Ok(None);
        }

        if LEFT_CLICKED.swap(false, Ordering::SeqCst) {
            cleanup_picker(window, window_dc, screen_dc);
            drop(mouse_hook);
            return Ok(Some(format_color(color, format)));
        }
        std::thread::sleep(std::time::Duration::from_millis(8));
    }
}

unsafe fn key_down(vk: i32) -> bool {
    (GetAsyncKeyState(vk) as u16 & 0x8000) != 0
}

unsafe extern "system" fn mouse_hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && wparam.0 == WM_LBUTTONDOWN as usize {
        LEFT_CLICKED.store(true, Ordering::SeqCst);
        return LRESULT(1);
    }
    unsafe { CallNextHookEx(HHOOK::default(), code, wparam, lparam) }
}

fn start_mouse_hook_thread() -> Result<MouseHookThread, String> {
    let (sender, receiver) = mpsc::channel::<Result<u32, String>>();
    let handle = thread::spawn(move || {
        let thread_id = unsafe { GetCurrentThreadId() };
        let hook = match unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook_proc), HINSTANCE::default(), 0) } {
            Ok(hook) => hook,
            Err(error) => {
                let _ = sender.send(Err(error.to_string()));
                return;
            }
        };
        let _ = sender.send(Ok(thread_id));

        unsafe {
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {}
            let _ = UnhookWindowsHookEx(hook);
        }
    });

    match receiver.recv().map_err(|error| error.to_string())? {
        Ok(thread_id) => Ok(MouseHookThread { thread_id, handle: Some(handle) }),
        Err(error) => {
            let _ = handle.join();
            Err(error)
        }
    }
}

fn cleanup_picker(window: HWND, window_dc: HDC, screen_dc: HDC) {
    unsafe {
        let _ = ReleaseDC(window, window_dc);
        let _ = ReleaseDC(None, screen_dc);
        let _ = DestroyWindow(window);
    }
}

fn sample_screen_pixel(screen_dc: HDC, x: i32, y: i32) -> Result<(u8, u8, u8), String> {
    unsafe {
        let raw = GetPixel(screen_dc, x, y);
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
    let mut clipboard = arboard::Clipboard::new().map_err(|error| error.to_string())?;
    clipboard
        .set_text(value.to_string())
        .map_err(|error| error.to_string())
}

fn update_picker_window(
    hwnd: HWND,
    window_dc: HDC,
    painter: &DoubleBuffer,
    screen_dc: HDC,
    x: i32,
    y: i32,
    color: (u8, u8, u8),
    format: Format,
) {
    let screen_w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let screen_h = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    let mut pos_x = x + 25;
    let mut pos_y = y + 25;
    if pos_x + WIN_W > screen_w {
        pos_x = x - WIN_W - 25;
    }
    if pos_y + WIN_H > screen_h {
        pos_y = y - WIN_H - 25;
    }
    if pos_x < 0 {
        pos_x = 0;
    }
    if pos_y < 0 {
        pos_y = 0;
    }

    unsafe {
        let _ = MoveWindow(hwnd, pos_x, pos_y, WIN_W, WIN_H, false);
        draw_picker(painter.mem_dc, screen_dc, x, y, color, format);
        let _ = BitBlt(window_dc, 0, 0, WIN_W, WIN_H, painter.mem_dc, 0, 0, SRCCOPY);
    }
}

fn draw_picker(
    hdc: HDC,
    screen_dc: HDC,
    x: i32,
    y: i32,
    color: (u8, u8, u8),
    format: Format,
) {
    unsafe {
        let bg = CreateSolidBrush(COLORREF(0x00111111));
        let _ = FillRect(hdc, &RECT { left: 0, top: 0, right: WIN_W, bottom: WIN_H }, bg);
        let _ = DeleteObject(bg);

        let _ = StretchBlt(hdc, 5, 10, 150, 150, screen_dc, x - 7, y - 7, 15, 15, SRCCOPY);

        let red = CreateSolidBrush(COLORREF(0x000000FF));
        for rect in [
            RECT { left: 74, top: 79, right: 86, bottom: 81 },
            RECT { left: 74, top: 89, right: 86, bottom: 91 },
            RECT { left: 74, top: 79, right: 76, bottom: 91 },
            RECT { left: 84, top: 79, right: 86, bottom: 91 },
        ] {
            let _ = FillRect(hdc, &rect, red);
        }
        let _ = DeleteObject(red);

        let (r, g, b) = color;
        let chip = CreateSolidBrush(COLORREF(u32::from(r) | (u32::from(g) << 8) | (u32::from(b) << 16)));
        let _ = FillRect(hdc, &RECT { left: 165, top: 10, right: 215, bottom: 60 }, chip);
        let _ = DeleteObject(chip);

        let _ = SetBkColor(hdc, COLORREF(0x00111111));
        let _ = SetTextColor(hdc, COLORREF(0x00FFFFFF));
        let hex = format_color(color, Format::Hex);
        let text = wide_no_null(&hex);
        let _ = TextOutW(hdc, 160, 72, &text);
        let preview = format_color(color, format);
        let first_line = preview.lines().next().unwrap_or(&preview);
        let text = wide_no_null(&truncate_text(first_line, 11));
        let _ = TextOutW(hdc, 160, 96, &text);
        let help1 = wide_no_null("Click copy");
        let help2 = wide_no_null("Esc cancel");
        let _ = SetTextColor(hdc, COLORREF(0x00AAAAAA));
        let _ = TextOutW(hdc, 160, 122, &help1);
        let _ = TextOutW(hdc, 160, 142, &help2);
    }
}

fn truncate_text(value: &str, max_chars: usize) -> String {
    let count = value.chars().count();
    if count <= max_chars {
        return value.to_string();
    }
    value.chars().take(max_chars.saturating_sub(1)).collect::<String>() + "…"
}

fn wide_no_null(value: &str) -> Vec<u16> {
    value.encode_utf16().collect()
}

fn create_picker_window() -> Result<HWND, String> {
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
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            class_name,
            w!("Color Picker"),
            WS_POPUP | WS_BORDER,
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
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
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
