//! Which app has focus.

use std::collections::HashMap;
use std::path::Path;

use ::windows::core::{BOOL, PCWSTR, PWSTR};
use ::windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
use ::windows::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};
use ::windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use ::windows::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, GetClassNameW, GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId,
};
use parking_lot::Mutex;

use crate::platform::FocusedApp;

/// Display names by executable path (reading the version resource is file I/O).
#[derive(Default)]
pub struct AppNames(Mutex<HashMap<String, String>>);

impl AppNames {
    pub fn focused_app(&self) -> Option<FocusedApp> {
        // SAFETY: plain query.
        let window = unsafe { GetForegroundWindow() };
        if window.is_invalid() {
            return None;
        }
        let (_, pid) = app_process(window);
        let path = process_path(pid)?;
        let id = Path::new(&path).file_name()?.to_string_lossy().into_owned();
        let name = {
            let mut names = self.0.lock();
            names.entry(path.clone()).or_insert_with(|| display_name(&path)).clone()
        };
        Some(FocusedApp { name, id, title: window_title(window) })
    }
}

/// The processes of the foreground window: the window's own, and for an app hosted in a frame
/// (Store apps run inside ApplicationFrameHost.exe) the hosted app's.
pub fn foreground_processes() -> [u32; 2] {
    // SAFETY: plain query.
    let window = unsafe { GetForegroundWindow() };
    if window.is_invalid() {
        return [0; 2];
    }
    let (frame, app) = app_process(window);
    [frame, app]
}

/// (the window's process, the app's process).
fn app_process(window: HWND) -> (u32, u32) {
    let mut pid = 0u32;
    // SAFETY: plain query with a valid out pointer.
    unsafe { GetWindowThreadProcessId(window, Some(&mut pid)) };
    let is_frame = process_path(pid).is_some_and(|path| {
        Path::new(&path).file_name().is_some_and(|name| name.eq_ignore_ascii_case("ApplicationFrameHost.exe"))
    });
    if !is_frame {
        return (pid, pid);
    }
    // The hosted app's window is a child of the frame, owned by another process.
    let mut search = (pid, 0u32);
    // SAFETY: the callback only runs during this call, while `search` is alive.
    unsafe {
        let _ = EnumChildWindows(Some(window), Some(find_hosted_app), LPARAM(&mut search as *mut (u32, u32) as isize));
    }
    (pid, if search.1 != 0 { search.1 } else { pid })
}

unsafe extern "system" fn find_hosted_app(child: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: `lparam` is the `(u32, u32)` passed by `app_process`, alive during enumeration.
    let search = unsafe { &mut *(lparam.0 as *mut (u32, u32)) };
    let mut pid = 0u32;
    // SAFETY: plain query with a valid out pointer.
    unsafe { GetWindowThreadProcessId(child, Some(&mut pid)) };
    if pid != 0 && pid != search.0 {
        search.1 = pid;
        return BOOL(0);
    }
    BOOL(1)
}

/// The full path of a process's executable.
fn process_path(pid: u32) -> Option<String> {
    if pid == 0 {
        return None;
    }
    // SAFETY: the handle is closed below; the buffer is valid for `len` units.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buffer = vec![0u16; 1024];
        let mut len = buffer.len() as u32;
        let result = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buffer.as_mut_ptr()), &mut len);
        let _ = CloseHandle(process);
        result.ok()?;
        Some(String::from_utf16_lossy(&buffer[..len as usize]))
    }
}

fn window_title(window: HWND) -> String {
    // GetWindowText reads the caption Windows keeps for windows of other processes, so it
    // can't hang on an unresponsive app.
    // SAFETY: the buffer is valid for its length.
    unsafe {
        let len = GetWindowTextLengthW(window);
        if len <= 0 {
            return String::new();
        }
        let mut buffer = vec![0u16; len as usize + 1];
        let copied = GetWindowTextW(window, &mut buffer);
        String::from_utf16_lossy(&buffer[..copied.max(0) as usize])
    }
}

/// The app's name from its version resource ("Visual Studio Code"), else the file name
/// without extension.
fn display_name(path: &str) -> String {
    let stem = Path::new(path).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    file_description(path).filter(|name| !name.trim().is_empty()).map(|name| name.trim().to_string()).unwrap_or(stem)
}

fn file_description(path: &str) -> Option<String> {
    let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
    let path = PCWSTR(wide.as_ptr());
    // SAFETY: `path` is NUL-terminated; `data` is sized as Windows asks, and the values read
    // from it are bounded by the lengths VerQueryValueW returns.
    unsafe {
        let size = GetFileVersionInfoSizeW(path, None);
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        GetFileVersionInfoW(path, None, size, data.as_mut_ptr().cast()).ok()?;

        let query = |sub_block: &str| -> Option<(*const u8, u32)> {
            let block: Vec<u16> = sub_block.encode_utf16().chain(Some(0)).collect();
            let mut pointer = std::ptr::null_mut();
            let mut len = 0u32;
            VerQueryValueW(data.as_ptr().cast(), PCWSTR(block.as_ptr()), &mut pointer, &mut len)
                .as_bool()
                .then_some((pointer as *const u8, len))
                .filter(|(pointer, len)| !pointer.is_null() && *len > 0)
        };

        let mut languages: Vec<(u16, u16)> = Vec::new();
        if let Some((pointer, len)) = query("\\VarFileInfo\\Translation") {
            let pairs = std::slice::from_raw_parts(pointer as *const u16, (len as usize / 2) & !1);
            languages.extend(pairs.as_chunks::<2>().0.iter().map(|&[language, codepage]| (language, codepage)));
        }
        // Common fallbacks: US English with Unicode or Windows-1252.
        languages.extend([(0x0409, 0x04B0), (0x0409, 0x04E4)]);
        for (language, codepage) in languages {
            let block = format!("\\StringFileInfo\\{language:04x}{codepage:04x}\\FileDescription");
            if let Some((pointer, len)) = query(&block) {
                // `len` counts UTF-16 units, including the terminating NUL.
                let units = std::slice::from_raw_parts(pointer as *const u16, len as usize);
                let end = units.iter().position(|&unit| unit == 0).unwrap_or(units.len());
                return Some(String::from_utf16_lossy(&units[..end]));
            }
        }
        None
    }
}

/// Window class name (used to recognize the desktop and the shell's own windows).
pub fn class_name(window: HWND) -> String {
    let mut buffer = [0u16; 128];
    // SAFETY: the buffer is valid for its length.
    let len = unsafe { GetClassNameW(window, &mut buffer) };
    String::from_utf16_lossy(&buffer[..len.max(0) as usize])
}
