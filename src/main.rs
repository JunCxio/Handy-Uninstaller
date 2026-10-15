#![cfg_attr(not(test), windows_subsystem = "windows")]

use std::collections::HashSet;
use std::ffi::c_void;
use std::io::Read;
use std::mem::{size_of, transmute, zeroed};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

type Handle = isize;
type Hwnd = Handle;
type Hkey = Handle;

const UNINSTALL_KEY: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall";
const KEY_READ: u32 = 0x20019;
const KEY_WRITE: u32 = 0x20006;
const VIEW_64: u32 = 0x100;
const VIEW_32: u32 = 0x200;
const REG_SZ: u32 = 1;
const REG_EXPAND_SZ: u32 = 2;
const REG_DWORD: u32 = 4;
const ERROR_NO_MORE_ITEMS: i32 = 259;

const WS_CHILD: u32 = 0x40000000;
const WS_VISIBLE: u32 = 0x10000000;
const WS_TABSTOP: u32 = 0x00010000;
const WS_OVERLAPPEDWINDOW: u32 = 0x00cf0000;
const LVS_REPORT: u32 = 1;
const LVS_SHOWSELALWAYS: u32 = 8;
const LVS_OWNERDRAWFIXED: u32 = 0x400;
const LVS_NOCOLUMNHEADER: u32 = 0x4000;
const BS_OWNERDRAW: u32 = 0x000b;
const ES_AUTOHSCROLL: u32 = 0x0080;
const WM_PAINT: u32 = 0x000f;
const WM_ERASEBKGND: u32 = 0x0014;
const WM_SIZE: u32 = 5;
const WM_DESTROY: u32 = 2;
const WM_CLOSE: u32 = 0x0010;
const WM_NCDESTROY: u32 = 0x0082;
const WM_COMMAND: u32 = 0x0111;
const WM_NOTIFY: u32 = 0x004e;
const WM_CTLCOLORSTATIC: u32 = 0x0138;
const WM_CTLCOLOREDIT: u32 = 0x0133;
const WM_CONTEXTMENU: u32 = 0x007b;
const WM_MEASUREITEM: u32 = 0x002c;
const WM_DRAWITEM: u32 = 0x002b;
const WM_LBUTTONDOWN: u32 = 0x0201;
const WM_GETMINMAXINFO: u32 = 0x0024;
const WM_SETFONT: u32 = 0x0030;
const WM_APP_LOADED: u32 = 0x8001;
const WM_APP_UNINSTALLED: u32 = 0x8002;
const WM_APP_MONITOR: u32 = 0x8003;
const WM_APP_DESKTOP_DONE: u32 = 0x8004;
const LVM_FIRST: u32 = 0x1000;
const LVM_SETBKCOLOR: u32 = LVM_FIRST + 1;
const LVM_SETIMAGELIST: u32 = LVM_FIRST + 3;
const LVM_DELETEALLITEMS: u32 = LVM_FIRST + 9;
const LVM_GETNEXTITEM: u32 = LVM_FIRST + 12;
const LVM_HITTEST: u32 = LVM_FIRST + 18;
const LVM_GETCOLUMNWIDTH: u32 = LVM_FIRST + 29;
const LVM_SETCOLUMNWIDTH: u32 = LVM_FIRST + 30;
const LVM_SETTEXTCOLOR: u32 = LVM_FIRST + 36;
const LVM_SETTEXTBKCOLOR: u32 = LVM_FIRST + 38;
const LVM_SETITEMSTATE: u32 = LVM_FIRST + 43;
const LVM_GETITEMSTATE: u32 = LVM_FIRST + 44;
const LVM_SETEXTENDEDLISTVIEWSTYLE: u32 = LVM_FIRST + 54;
const LVM_INSERTITEMW: u32 = LVM_FIRST + 77;
const LVM_INSERTCOLUMNW: u32 = LVM_FIRST + 97;
const LVM_SETITEMTEXTW: u32 = LVM_FIRST + 116;
const LVN_ITEMCHANGED: i32 = -101;
const ID_SEARCH: usize = 101;
const ID_REFRESH: usize = 102;
const ID_MONITOR: usize = 103;
const ID_DESKTOP_TAB: usize = 104;
const ID_STORE_TAB: usize = 105;
const ID_THEME: usize = 106;
const ID_LIST: usize = 107;
const MENU_UNINSTALL: u32 = 201;
const MENU_FORCE_REMOVE: u32 = 202;
const MENU_REMOVE_ENTRY: u32 = 203;
const MENU_REGISTRY: u32 = 204;
const MENU_FOLDER: u32 = 205;
const MENU_BATCH_UNINSTALL: u32 = 206;
const ID_REMOVE_CONFIRM: usize = 301;
const ID_REMOVE_CANCEL: usize = 302;

#[repr(C)]
#[derive(Clone, Copy)]
struct Point { x: i32, y: i32 }
#[repr(C)]
#[derive(Clone, Copy)]
struct Rect { left: i32, top: i32, right: i32, bottom: i32 }
#[repr(C)]
struct PaintStruct { dc: Handle, erase: i32, rect: Rect, restore: i32, update: i32, reserved: [u8; 32] }
#[repr(C)]
struct MeasureItem { kind: u32, id: u32, item: u32, width: u32, height: u32, data: usize }
#[repr(C)]
struct DrawItem { kind: u32, id: u32, item: u32, action: u32, state: u32,
    window: Hwnd, dc: Handle, rect: Rect, data: usize }
#[repr(C)]
struct MinMaxInfo { reserved: Point, max_size: Point, max_position: Point,
    min_track: Point, max_track: Point }
#[repr(C)]
struct Msg { hwnd: Hwnd, message: u32, wparam: usize, lparam: isize, time: u32, point: Point, private: u32 }
#[repr(C)]
struct WndClass {
    style: u32,
    proc: Option<unsafe extern "system" fn(Hwnd, u32, usize, isize) -> isize>,
    class_extra: i32,
    window_extra: i32,
    instance: Handle,
    icon: Handle,
    cursor: Handle,
    background: Handle,
    menu_name: *const u16,
    class_name: *const u16,
}
#[repr(C)]
struct InitCommonControls { size: u32, classes: u32 }
#[repr(C)]
struct LvColumn {
    mask: u32, format: i32, width: i32, text: *mut u16, text_max: i32,
    subitem: i32, image: i32, order: i32, min_width: i32, default_width: i32, ideal_width: i32,
}
#[repr(C)]
struct LvItem {
    mask: u32, item: i32, subitem: i32, state: u32, state_mask: u32,
    text: *mut u16, text_max: i32, image: i32, param: isize, indent: i32,
    group_id: i32, columns: u32, column_ptr: *mut u32, column_format: *mut i32, group: i32,
}
#[repr(C)]
struct NmHdr { hwnd: Hwnd, id: usize, code: i32 }
#[repr(C)]
struct LvHitTest { point: Point, flags: u32, item: i32, subitem: i32, group: i32 }

#[link(name = "advapi32")]
extern "system" {
    fn RegOpenKeyExW(key: Hkey, subkey: *const u16, options: u32, access: u32, result: *mut Hkey) -> i32;
    fn RegEnumKeyExW(key: Hkey, index: u32, name: *mut u16, len: *mut u32,
        reserved: *mut u32, class: *mut u16, class_len: *mut u32, time: *mut c_void) -> i32;
    fn RegQueryValueExW(key: Hkey, name: *const u16, reserved: *mut u32,
        kind: *mut u32, data: *mut u8, len: *mut u32) -> i32;
    fn RegCloseKey(key: Hkey) -> i32;
    fn RegCreateKeyExW(key: Hkey, subkey: *const u16, reserved: u32, class: *mut u16,
        options: u32, access: u32, security: *const c_void, result: *mut Hkey,
        disposition: *mut u32) -> i32;
    fn RegSetValueExW(key: Hkey, name: *const u16, reserved: u32, kind: u32,
        data: *const u8, len: u32) -> i32;
    fn RegDeleteTreeW(key: Hkey, subkey: *const u16) -> i32;
}
#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(name: *const u16) -> Handle;
    fn ExpandEnvironmentStringsW(source: *const u16, result: *mut u16, size: u32) -> u32;
    fn LoadLibraryW(name: *const u16) -> Handle;
    fn GetProcAddress(module: Handle, name: *const u8) -> *const c_void;
    fn FreeLibrary(module: Handle) -> i32;
}
#[link(name = "user32")]
extern "system" {
    fn RegisterClassW(class: *const WndClass) -> u16;
    fn CreateWindowExW(ex_style: u32, class: *const u16, title: *const u16,
        style: u32, x: i32, y: i32, width: i32, height: i32,
        parent: Hwnd, menu: Handle, instance: Handle, param: *mut c_void) -> Hwnd;
    fn DefWindowProcW(window: Hwnd, message: u32, wparam: usize, lparam: isize) -> isize;
    fn ShowWindow(window: Hwnd, command: i32) -> i32;
    fn UpdateWindow(window: Hwnd) -> i32;
    fn GetMessageW(message: *mut Msg, window: Hwnd, min: u32, max: u32) -> i32;
    fn TranslateMessage(message: *const Msg) -> i32;
    fn DispatchMessageW(message: *const Msg) -> isize;
    fn PostQuitMessage(code: i32);
    fn PostMessageW(window: Hwnd, message: u32, wparam: usize, lparam: isize) -> i32;
    fn SendMessageW(window: Hwnd, message: u32, wparam: usize, lparam: isize) -> isize;
    fn SetWindowLongPtrW(window: Hwnd, index: i32, value: isize) -> isize;
    fn GetWindowLongPtrW(window: Hwnd, index: i32) -> isize;
    fn GetClientRect(window: Hwnd, rect: *mut Rect) -> i32;
    fn GetWindowRect(window: Hwnd, rect: *mut Rect) -> i32;
    fn MoveWindow(window: Hwnd, x: i32, y: i32, width: i32, height: i32, repaint: i32) -> i32;
    fn DestroyWindow(window: Hwnd) -> i32;
    fn BeginPaint(window: Hwnd, paint: *mut PaintStruct) -> Handle;
    fn EndPaint(window: Hwnd, paint: *const PaintStruct) -> i32;
    fn InvalidateRect(window: Hwnd, rect: *const Rect, erase: i32) -> i32;
    fn SetFocus(window: Hwnd) -> Hwnd;
    fn FillRect(dc: Handle, rect: *const Rect, brush: Handle) -> i32;
    fn DrawTextW(dc: Handle, text: *const u16, length: i32, rect: *mut Rect, flags: u32) -> i32;
    fn EnableWindow(window: Hwnd, enable: i32) -> i32;
    fn SetWindowTextW(window: Hwnd, text: *const u16) -> i32;
    fn MessageBoxW(window: Hwnd, text: *const u16, title: *const u16, flags: u32) -> i32;
    fn CreatePopupMenu() -> Handle;
    fn AppendMenuW(menu: Handle, flags: u32, item: usize, text: *const u16) -> i32;
    fn TrackPopupMenu(menu: Handle, flags: u32, x: i32, y: i32, reserved: i32,
        window: Hwnd, rect: *const Rect) -> i32;
    fn DestroyMenu(menu: Handle) -> i32;
    fn GetCursorPos(point: *mut Point) -> i32;
    fn ScreenToClient(window: Hwnd, point: *mut Point) -> i32;
    fn LoadIconW(instance: Handle, name: *const u16) -> Handle;
    fn LoadCursorW(instance: Handle, name: *const u16) -> Handle;
    fn DestroyIcon(icon: Handle) -> i32;
}
#[link(name = "gdi32")]
extern "system" {
    fn GetStockObject(index: i32) -> Handle;
    fn CreateFontW(height: i32, width: i32, escapement: i32, orientation: i32, weight: i32,
        italic: u32, underline: u32, strike: u32, charset: u32, output: u32, clip: u32,
        quality: u32, pitch: u32, face: *const u16) -> Handle;
    fn CreateSolidBrush(color: u32) -> Handle;
    fn DeleteObject(object: Handle) -> i32;
    fn SetTextColor(dc: Handle, color: u32) -> u32;
    fn SetBkColor(dc: Handle, color: u32) -> u32;
    fn SetBkMode(dc: Handle, mode: i32) -> i32;
    fn SelectObject(dc: Handle, object: Handle) -> Handle;
    fn CreatePen(style: i32, width: i32, color: u32) -> Handle;
    fn RoundRect(dc: Handle, left: i32, top: i32, right: i32, bottom: i32,
        ellipse_width: i32, ellipse_height: i32) -> i32;
    fn Ellipse(dc: Handle, left: i32, top: i32, right: i32, bottom: i32) -> i32;
    fn MoveToEx(dc: Handle, x: i32, y: i32, previous: *mut Point) -> i32;
    fn LineTo(dc: Handle, x: i32, y: i32) -> i32;
}
#[link(name = "comctl32")]
extern "system" {
    fn InitCommonControlsEx(controls: *const InitCommonControls) -> i32;
    fn ImageList_Create(width: i32, height: i32, flags: u32, initial: i32, grow: i32) -> Handle;
    fn ImageList_ReplaceIcon(list: Handle, index: i32, icon: Handle) -> i32;
    fn ImageList_Remove(list: Handle, index: i32) -> i32;
    fn ImageList_Destroy(list: Handle) -> i32;
    fn ImageList_Draw(list: Handle, index: i32, dc: Handle, x: i32, y: i32, style: u32) -> i32;
}
#[link(name = "shell32")]
extern "system" {
    fn ExtractIconExW(file: *const u16, index: i32, large: *mut Handle, small: *mut Handle, count: u32) -> u32;
    fn ShellExecuteW(window: Hwnd, verb: *const u16, file: *const u16,
        arguments: *const u16, directory: *const u16, show: i32) -> Handle;
}
#[repr(C)]
struct GdiplusStartupInput {
    version: u32, callback: *const c_void, suppress_background_thread: i32, suppress_external_codecs: i32,
}
struct GdiPlus {
    module: Handle, token: usize,
    create_bitmap: unsafe extern "system" fn(*const u16, *mut *mut c_void) -> i32,
    create_icon: unsafe extern "system" fn(*mut c_void, *mut Handle) -> i32,
    dispose_image: unsafe extern "system" fn(*mut c_void) -> i32,
    shutdown: unsafe extern "system" fn(usize),
}

impl GdiPlus {
    fn start() -> Option<Self> {
        unsafe {
            let module = LoadLibraryW(wide("gdiplus.dll").as_ptr());
            if module == 0 { return None; }
            let names = [b"GdiplusStartup\0".as_slice(), b"GdiplusShutdown\0",
                b"GdipCreateBitmapFromFile\0", b"GdipCreateHICONFromBitmap\0", b"GdipDisposeImage\0"];
            let functions = names.map(|name| GetProcAddress(module, name.as_ptr()));
            if functions.iter().any(|function| function.is_null()) {
                FreeLibrary(module);
                return None;
            }
            let startup: unsafe extern "system" fn(*mut usize, *const GdiplusStartupInput, *mut c_void) -> i32 =
                transmute(functions[0]);
            let mut token = 0;
            let input = GdiplusStartupInput { version: 1, callback: null(),
                suppress_background_thread: 0, suppress_external_codecs: 0 };
            if startup(&mut token, &input, null_mut()) != 0 {
                FreeLibrary(module);
                return None;
            }
            Some(Self { module, token, create_bitmap: transmute(functions[2]),
                create_icon: transmute(functions[3]), dispose_image: transmute(functions[4]),
                shutdown: transmute(functions[1]) })
        }
    }
}

impl Drop for GdiPlus {
    fn drop(&mut self) {
        unsafe { (self.shutdown)(self.token); FreeLibrary(self.module); }
    }
}

fn wide(text: &str) -> Vec<u16> { text.encode_utf16().chain(std::iter::once(0)).collect() }
fn color(red: u8, green: u8, blue: u8) -> u32 {
    red as u32 | ((green as u32) << 8) | ((blue as u32) << 16)
}
#[derive(Clone, Copy)]
struct Palette { background: u32, chrome: u32, text: u32, muted: u32, line: u32,
    accent: u32, selected: u32, input: u32, badge: u32 }

fn palette(dark: bool) -> Palette {
    if dark {
        Palette { background: color(15, 23, 36), chrome: color(18, 30, 46),
            text: color(230, 240, 251), muted: color(148, 169, 193),
            line: color(42, 59, 80), accent: color(105, 172, 255),
            selected: color(30, 54, 84), input: color(25, 39, 57), badge: color(35, 53, 75) }
    } else {
        Palette { background: color(255, 255, 255), chrome: color(248, 250, 253),
            text: color(24, 45, 72), muted: color(104, 128, 158),
            line: color(228, 235, 244), accent: color(22, 111, 217),
            selected: color(234, 244, 255), input: color(255, 255, 255),
            badge: color(242, 246, 251) }
    }
}

fn fill(dc: Handle, rect: Rect, shade: u32) {
    unsafe {
        let brush = CreateSolidBrush(shade);
        FillRect(dc, &rect, brush);
        DeleteObject(brush);
    }
}

fn rounded(dc: Handle, rect: Rect, background: u32, border: u32, radius: i32) {
    unsafe {
        let brush = CreateSolidBrush(background);
        let pen = CreatePen(0, 1, border);
        let old_brush = SelectObject(dc, brush);
        let old_pen = SelectObject(dc, pen);
        RoundRect(dc, rect.left, rect.top, rect.right, rect.bottom, radius, radius);
        SelectObject(dc, old_pen);
        SelectObject(dc, old_brush);
        DeleteObject(pen);
        DeleteObject(brush);
    }
}

fn draw_text(dc: Handle, text: &str, mut rect: Rect, font: Handle, shade: u32, flags: u32) {
    unsafe {
        let old = SelectObject(dc, font);
        SetBkMode(dc, 1);
        SetTextColor(dc, shade);
        DrawTextW(dc, wide(text).as_ptr(), -1, &mut rect, flags | 0x800);
        SelectObject(dc, old);
    }
}

fn saved_dark_mode() -> bool {
    open_key(hive(false), "SOFTWARE\\HandyUninstaller", 0, KEY_READ)
        .is_some_and(|key| is_one(key.0, "DarkMode"))
}

fn save_dark_mode(dark: bool) -> Result<(), i32> {
    let mut handle = 0;
    let result = unsafe { RegCreateKeyExW(hive(false), wide("SOFTWARE\\HandyUninstaller").as_ptr(),
        0, null_mut(), 0, KEY_WRITE, null(), &mut handle, null_mut()) };
    if result != 0 { return Err(result); }
    let key = Key(handle);
    let value = u32::from(dark).to_le_bytes();
    let result = unsafe { RegSetValueExW(key.0, wide("DarkMode").as_ptr(), 0, REG_DWORD,
        value.as_ptr(), value.len() as u32) };
    if result == 0 { Ok(()) } else { Err(result) }
}

fn set_title_dark(window: Hwnd, dark: bool) {
    let value = i32::from(dark);
    unsafe {
        let module = LoadLibraryW(wide("dwmapi.dll").as_ptr());
        if module == 0 { return; }
        let address = GetProcAddress(module, b"DwmSetWindowAttribute\0".as_ptr());
        if !address.is_null() {
            let set_attribute: unsafe extern "system" fn(Hwnd, u32, *const c_void, u32) -> i32 = transmute(address);
            if set_attribute(window, 20, &value as *const _ as *const c_void, 4) != 0 {
                set_attribute(window, 19, &value as *const _ as *const c_void, 4);
            }
        }
        FreeLibrary(module);
    }
}

fn set_control_theme(window: Hwnd, dark: bool) {
    unsafe {
        let module = LoadLibraryW(wide("uxtheme.dll").as_ptr());
        if module == 0 { return; }
        let address = GetProcAddress(module, b"SetWindowTheme\0".as_ptr());
        if !address.is_null() {
            let set_theme: unsafe extern "system" fn(Hwnd, *const u16, *const u16) -> i32 = transmute(address);
            let name = wide("DarkMode_Explorer");
            set_theme(window, if dark { name.as_ptr() } else { null() }, null());
        }
        FreeLibrary(module);
    }
}
fn from_wide(text: &[u16]) -> String {
    String::from_utf16_lossy(&text[..text.iter().position(|&ch| ch == 0).unwrap_or(text.len())])
}
fn hive(machine: bool) -> Hkey { (if machine { 0x80000002u32 } else { 0x80000001u32 }) as i32 as isize }

struct Key(Hkey);
impl Drop for Key { fn drop(&mut self) { unsafe { RegCloseKey(self.0); } } }

fn open_key(root: Hkey, path: &str, view: u32, access: u32) -> Option<Key> {
    let mut key = 0;
    let status = unsafe { RegOpenKeyExW(root, wide(path).as_ptr(), 0, access | view, &mut key) };
    (status == 0).then_some(Key(key))
}

fn value(key: Hkey, name: &str) -> Option<(u32, Vec<u8>)> {
    let name = wide(name);
    let (mut kind, mut len) = (0, 0);
    if unsafe { RegQueryValueExW(key, name.as_ptr(), null_mut(), &mut kind, null_mut(), &mut len) } != 0 { return None; }
    let mut bytes = vec![0; len as usize];
    if unsafe { RegQueryValueExW(key, name.as_ptr(), null_mut(), &mut kind, bytes.as_mut_ptr(), &mut len) } != 0 { return None; }
    bytes.truncate(len as usize);
    Some((kind, bytes))
}

fn text_value(key: Hkey, name: &str) -> String {
    match value(key, name) {
        Some((REG_SZ | REG_EXPAND_SZ, bytes)) => {
            let chars: Vec<u16> = bytes.chunks_exact(2).map(|part| u16::from_le_bytes([part[0], part[1]])).collect();
            from_wide(&chars)
        }
        _ => String::new(),
    }
}

fn is_one(key: Hkey, name: &str) -> bool {
    matches!(value(key, name), Some((REG_DWORD, bytes)) if bytes.len() >= 4 && u32::from_le_bytes(bytes[..4].try_into().unwrap()) == 1)
}

#[derive(Clone)]
struct Entry {
    name: String, version: String, publisher: String, source: String,
    command: String, subkey: String, package: String, package_family: String,
    display_icon: String, install_location: String,
    install_date: String, machine: bool, view: u32, msi: bool, image: i32,
}

fn install_date(text: &str) -> String {
    let bytes = text.as_bytes();
    if bytes.len() != 8 || !bytes.iter().all(u8::is_ascii_digit) { return String::new(); }
    let year = text[..4].parse::<u16>().unwrap();
    let month = text[4..6].parse::<u8>().unwrap();
    let day = text[6..8].parse::<u8>().unwrap();
    if year == 0 || !(1..=12).contains(&month) { return String::new(); }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    if day == 0 || day > days[month as usize - 1] { return String::new(); }
    format!("{}-{}-{}", &text[..4], &text[4..6], &text[6..])
}

fn include(name: &str, command: &str, system: bool, parent: &str) -> bool {
    !name.trim().is_empty() && !command.trim().is_empty() && !system && parent.trim().is_empty()
}

fn registry_apps(errors: &mut Vec<String>) -> Vec<Entry> {
    let mut entries = Vec::new();
    let mut seen = HashSet::new();
    for machine in [true, false] {
        for (view, width) in [(VIEW_64, "64 位"), (VIEW_32, "32 位")] {
            let source = format!("{} {width}", if machine { "HKLM" } else { "HKCU" });
            let Some(root) = open_key(hive(machine), UNINSTALL_KEY, view, KEY_READ) else { continue };
            let mut index = 0;
            loop {
                let mut name = vec![0u16; 32768];
                let mut len = name.len() as u32;
                let status = unsafe { RegEnumKeyExW(root.0, index, name.as_mut_ptr(), &mut len,
                    null_mut(), null_mut(), null_mut(), null_mut()) };
                if status == ERROR_NO_MORE_ITEMS { break; }
                index += 1;
                if status != 0 { errors.push(format!("{source} 枚举失败：{status}")); continue; }
                let subkey = from_wide(&name[..len as usize]);
                let Some(key) = open_key(root.0, &subkey, view, KEY_READ) else { continue };
                let display_name = text_value(key.0, "DisplayName").trim().to_owned();
                let command = text_value(key.0, "UninstallString").trim().to_owned();
                if !include(&display_name, &command, is_one(key.0, "SystemComponent"),
                    &text_value(key.0, "ParentDisplayName")) { continue; }
                let identity = format!("{}\0{}\0{}", if machine { "HKLM" } else { "HKCU" },
                    subkey.to_lowercase(), command.to_lowercase());
                if !seen.insert(identity) { continue; }
                entries.push(Entry {
                    name: display_name, version: text_value(key.0, "DisplayVersion").trim().to_owned(),
                    publisher: text_value(key.0, "Publisher").trim().to_owned(), source: source.clone(),
                    command, subkey, package: String::new(), package_family: String::new(),
                    display_icon: text_value(key.0, "DisplayIcon").trim().to_owned(),
                    install_location: text_value(key.0, "InstallLocation").trim().to_owned(),
                    install_date: install_date(text_value(key.0, "InstallDate").trim()),
                    machine, view, msi: is_one(key.0, "WindowsInstaller"), image: 0,
                });
            }
        }
    }
    entries
}

fn store_apps(errors: &mut Vec<String>) -> Vec<Entry> {
    let script = include_str!("../assets/store_apps.ps1");
    let child = Command::new("powershell.exe").args(["-NoProfile", "-NonInteractive", "-Command", script])
        .creation_flags(0x08000000).stdout(Stdio::piped()).stderr(Stdio::null()).spawn();
    let Ok(mut child) = child else { errors.push("商店应用读取失败：无法启动 PowerShell".into()); return Vec::new() };
    let mut stdout = child.stdout.take().unwrap();
    let reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout.read_to_end(&mut bytes);
        bytes
    });
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() < Duration::from_secs(30) => thread::sleep(Duration::from_millis(100)),
            _ => { let _ = child.kill(); errors.push("商店应用读取超时或失败".into()); break; }
        }
    }
    let status = child.wait();
    let bytes = reader.join().unwrap_or_default();
    let Ok(status) = status else { return Vec::new() };
    if !status.success() {
        errors.push(format!("商店应用读取失败：PowerShell 退出代码 {:?}", status.code()));
        return Vec::new();
    }
    String::from_utf8_lossy(&bytes).lines().filter_map(|line| {
        let mut fields = line.split('\t');
        let (name, package, family, version, location, icon) =
            (fields.next()?, fields.next()?, fields.next()?, fields.next()?, fields.next()?, fields.next()?);
        if name.is_empty() || package.is_empty() || !single_key_component(family)
            || fields.next().is_some() { return None; }
        Some(Entry { name: name.into(), version: version.into(), publisher: String::new(),
            source: "商店应用".into(), command: String::new(), subkey: String::new(),
            package: package.into(), package_family: family.into(),
            display_icon: icon.into(), install_location: location.into(),
            install_date: String::new(), machine: false, view: 0, msi: false, image: 0 })
    }).collect()
}

fn inventory() -> (Vec<Entry>, Vec<String>) {
    let mut errors = Vec::new();
    let mut entries = registry_apps(&mut errors);
    entries.extend(store_apps(&mut errors));
    entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    (entries, errors)
}

fn entry_identity(entry: &Entry) -> String {
    if entry.package.is_empty() {
        format!("{}:{}:{}", entry.machine, entry.view, entry.subkey.to_lowercase())
    } else { entry.package.to_lowercase() }
}

fn in_view(package: &str, store_view: bool) -> bool {
    package.is_empty() != store_view
}

fn entry_order(a: &Entry, b: &Entry, column: usize, descending: bool) -> std::cmp::Ordering {
    let left = match column {
        1 => &a.version, 2 => &a.publisher, 3 => &a.source, 4 => &a.install_date,
        _ => &a.name,
    };
    let right = match column {
        1 => &b.version, 2 => &b.publisher, 3 => &b.source, 4 => &b.install_date,
        _ => &b.name,
    };
    let order = match (left.is_empty(), right.is_empty()) {
        (true, false) => std::cmp::Ordering::Greater,
        (false, true) => std::cmp::Ordering::Less,
        _ => {
            let order = left.to_lowercase().cmp(&right.to_lowercase());
            if descending { order.reverse() } else { order }
        }
    };
    order.then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
}

fn expand_env(text: &str) -> String {
    let input = wide(text);
    let needed = unsafe { ExpandEnvironmentStringsW(input.as_ptr(), null_mut(), 0) };
    if needed == 0 { return text.into(); }
    let mut buffer = vec![0; needed as usize];
    if unsafe { ExpandEnvironmentStringsW(input.as_ptr(), buffer.as_mut_ptr(), needed) } == 0 { return text.into(); }
    from_wide(&buffer)
}

fn parse_display_icon(text: &str) -> (String, i32) {
    let expanded = expand_env(text.trim());
    let (path, index) = match expanded.rfind(',') {
        Some(comma) => match expanded[comma + 1..].trim().parse::<i32>() {
            Ok(index) => (&expanded[..comma], index),
            Err(_) => (expanded.as_str(), 0),
        },
        None => (expanded.as_str(), 0),
    };
    (path.trim().trim_matches('"').to_owned(), index)
}

fn split_command(command: &str) -> Result<(String, String), String> {
    let expanded = expand_env(command.trim());
    if let Some(rest) = expanded.strip_prefix('"') {
        let Some(end) = rest.find('"') else { return Err("卸载命令缺少结束引号".into()) };
        return Ok((rest[..end].into(), rest[end + 1..].trim_start().into()));
    }
    let bytes = expanded.as_bytes();
    let split = bytes.windows(4).enumerate().find(|(i, part)|
        part.eq_ignore_ascii_case(b".exe") && (*i + 4 == bytes.len() || bytes[*i + 4].is_ascii_whitespace()))
        .map(|(i, _)| i + 4)
        .or_else(|| expanded.find(char::is_whitespace)).unwrap_or(expanded.len());
    if split == 0 { return Err("卸载命令没有可执行文件".into()); }
    Ok((expanded[..split].into(), expanded[split..].trim_start().into()))
}

fn msi_command(command: &str) -> String {
    let bytes = command.as_bytes();
    for i in 1..bytes.len().saturating_sub(1) {
        if bytes[i - 1].is_ascii_whitespace() && (bytes[i] == b'/' || bytes[i] == b'-')
            && (bytes[i + 1] == b'I' || bytes[i + 1] == b'i')
            && (i + 2 == bytes.len() || bytes[i + 2].is_ascii_whitespace() || bytes[i + 2] == b'{') {
            let mut result = command.to_owned();
            result.replace_range(i..i + 2, "/X");
            return result;
        }
    }
    command.into()
}

fn install_folder(entry: &Entry) -> Option<String> {
    let location = expand_env(entry.install_location.trim().trim_matches('"'));
    if Path::new(&location).is_absolute() && Path::new(&location).is_dir() { return Some(location); }
    let (icon, _) = parse_display_icon(&entry.display_icon);
    if !Path::new(&icon).is_absolute() { return None; }
    let folder = Path::new(&icon).parent()?.canonicalize().ok()?;
    if folder.components().count() < 3 { return None; }
    if std::env::var_os("WINDIR").and_then(|root| PathBuf::from(root).canonicalize().ok())
        .is_some_and(|root| folder.starts_with(root)) { return None; }
    Some(folder.to_string_lossy().into_owned())
}

fn registry_display_path(entry: &Entry) -> String {
    let root = if entry.machine { "HKEY_LOCAL_MACHINE" } else { "HKEY_CURRENT_USER" };
    let path = if entry.machine && entry.view == VIEW_32 {
        UNINSTALL_KEY.replacen("SOFTWARE\\", "SOFTWARE\\Wow6432Node\\", 1)
    } else { UNINSTALL_KEY.into() };
    format!("Computer\\{root}\\{path}\\{}", entry.subkey)
}

#[derive(Clone, PartialEq, Eq)]
enum RemovalTarget {
    Directory(PathBuf),
    Shortcut(PathBuf),
    Registry { machine: bool, view: u32, parent: String, name: String },
}

impl RemovalTarget {
    fn exists(&self) -> bool {
        match self {
            Self::Directory(path) => path.is_dir(),
            Self::Shortcut(path) => path.is_file(),
            Self::Registry { machine, view, parent, name } =>
                open_key(hive(*machine), &format!("{parent}\\{name}"), *view, KEY_READ).is_some(),
        }
    }

    fn description(&self) -> String {
        match self {
            Self::Directory(path) => format!("文件夹：{}", display_path(path)),
            Self::Shortcut(path) => format!("快捷方式：{}", display_path(path)),
            Self::Registry { machine, view, parent, name } => format!("注册表：{}\\{}\\{}（{} 位）",
                if *machine { "HKLM" } else { "HKCU" }, parent, name,
                if *view == VIEW_32 { 32 } else { 64 }),
        }
    }

    fn delete(&self) -> Result<(), String> {
        match self {
            Self::Directory(path) => std::fs::remove_dir_all(path).map_err(|error| error.to_string()),
            Self::Shortcut(path) => std::fs::remove_file(path).map_err(|error| error.to_string()),
            Self::Registry { machine, view, parent, name } => {
                let key = open_key(hive(*machine), parent, *view, KEY_READ | KEY_WRITE | 0x10000)
                    .ok_or_else(|| "无法写入注册表，请以管理员身份运行后重试".to_owned())?;
                let status = unsafe { RegDeleteTreeW(key.0, wide(name).as_ptr()) };
                if status == 0 { Ok(()) } else { Err(format!("注册表错误代码：{status}")) }
            }
        }
    }
}

fn display_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") { format!(r"\\{rest}") }
    else { text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned() }
}

fn single_key_component(name: &str) -> bool {
    !name.is_empty() && !name.contains(['\\', '/', '\0']) && name != "." && name != ".."
}

fn safe_directory(path: &Path, entry: &Entry, entries: &[Entry]) -> Option<PathBuf> {
    if !path.is_absolute() { return None; }
    let path = path.canonicalize().ok()?;
    if !path.is_dir() || path.components().count() < 3 { return None; }
    for variable in ["WINDIR", "SystemRoot", "ProgramFiles", "ProgramFiles(x86)", "CommonProgramFiles",
        "CommonProgramFiles(x86)", "ProgramData", "PUBLIC", "USERPROFILE", "APPDATA",
        "LOCALAPPDATA", "TEMP", "TMP"] {
        if let Some(root) = std::env::var_os(variable).and_then(|value| PathBuf::from(value).canonicalize().ok()) {
            if path == root || (matches!(variable, "WINDIR" | "SystemRoot") && path.starts_with(root)) { return None; }
        }
    }
    if let Some(home) = std::env::var_os("USERPROFILE").map(PathBuf::from) {
        for name in ["Desktop", "Documents", "Downloads"] {
            if home.join(name).canonicalize().ok().as_ref() == Some(&path) { return None; }
        }
    }
    for other in entries {
        if let Some(other_path) = install_folder(other).and_then(|path| PathBuf::from(path).canonicalize().ok()) {
            let same_entry = other.machine == entry.machine && other.view == entry.view
                && other.subkey == entry.subkey && other.package == entry.package;
            if !same_entry && other_path.starts_with(&path) { return None; }
        }
    }
    Some(path)
}

fn store_data_directory(entry: &Entry, entries: &[Entry], local_app_data: &Path) -> Option<PathBuf> {
    if !single_key_component(&entry.package_family) { return None; }
    let root = local_app_data.join("Packages").canonicalize().ok()?;
    let folder = safe_directory(&root.join(&entry.package_family), entry, entries)?;
    folder.starts_with(root).then_some(folder)
}

fn removal_candidates(entry: &Entry, entries: &[Entry]) -> Vec<RemovalTarget> {
    if !entry.package.is_empty() {
        let mut candidates = Vec::new();
        if let Some(folder) = std::env::var_os("LOCALAPPDATA").and_then(|root|
            store_data_directory(entry, entries, Path::new(&root))) {
            candidates.push(RemovalTarget::Directory(folder));
        }
        for (parent, name) in [
            ("SOFTWARE\\Classes\\Local Settings\\Software\\Microsoft\\Windows\\CurrentVersion\\AppModel\\SystemAppData", &entry.package_family),
            ("SOFTWARE\\Classes\\ActivatableClasses\\Package", &entry.package),
        ] {
            if single_key_component(name) && open_key(hive(false), &format!("{parent}\\{name}"), VIEW_64, KEY_READ).is_some() {
                candidates.push(RemovalTarget::Registry { machine: false, view: VIEW_64,
                    parent: parent.into(), name: name.clone() });
            }
        }
        return candidates;
    }
    let mut candidates = Vec::new();
    let install = install_folder(entry).and_then(|location|
        safe_directory(Path::new(&location), entry, entries));
    if let Some(folder) = &install {
        let script = "$ErrorActionPreference='SilentlyContinue'; [Console]::OutputEncoding=[Text.Encoding]::UTF8; $s=New-Object -ComObject WScript.Shell; @([Environment]::GetFolderPath('Programs'),[Environment]::GetFolderPath('CommonPrograms'),[Environment]::GetFolderPath('DesktopDirectory'),[Environment]::GetFolderPath('CommonDesktopDirectory')) | Where-Object { $_ -and (Test-Path -LiteralPath $_) } | Select-Object -Unique | ForEach-Object { Get-ChildItem -LiteralPath $_ -Filter *.lnk -File -Recurse } | ForEach-Object { $_.FullName + [char]9 + $s.CreateShortcut($_.FullName).TargetPath }";
        if let Ok(output) = Command::new("powershell.exe").args(["-NoProfile", "-NonInteractive", "-Command", script])
            .creation_flags(0x08000000).output() {
            if output.status.success() {
                let mut seen = HashSet::new();
                for line in String::from_utf8_lossy(&output.stdout).lines() {
                    let Some((link, target)) = line.split_once('\t') else { continue };
                    if !Path::new(target).is_absolute() { continue; }
                    let Some(target) = Path::new(target).canonicalize().ok() else { continue };
                    if target.starts_with(folder) {
                        let link = PathBuf::from(link);
                        if link.is_file() && seen.insert(link.clone()) { candidates.push(RemovalTarget::Shortcut(link)); }
                    }
                }
            }
        }
    }
    if let Some(folder) = &install { candidates.push(RemovalTarget::Directory(folder.clone())); }
    let specific_name = single_key_component(&entry.name) && entry.name.chars().count() >= 4
        && !entry.publisher.to_lowercase().contains(&entry.name.to_lowercase());
    if specific_name {
        for variable in ["APPDATA", "LOCALAPPDATA", "PROGRAMDATA"] {
            if let Some(root) = std::env::var_os(variable) {
                let path = PathBuf::from(root).join(&entry.name);
                if let Some(folder) = safe_directory(&path, entry, entries) {
                    if install.as_ref() != Some(&folder) { candidates.push(RemovalTarget::Directory(folder)); }
                }
            }
        }
        for machine in [false, true] {
            for view in [VIEW_64, VIEW_32] {
                if !machine && view == VIEW_32 { continue; }
                for parent in ["SOFTWARE".to_owned(), format!("SOFTWARE\\{}", entry.publisher)] {
                    if parent != "SOFTWARE" && !single_key_component(&entry.publisher) { continue; }
                    let path = format!("{parent}\\{}", entry.name);
                    if open_key(hive(machine), &path, view, KEY_READ).is_some() {
                        candidates.push(RemovalTarget::Registry { machine, view,
                            parent, name: entry.name.clone() });
                    }
                }
            }
        }
    }
    candidates.push(RemovalTarget::Registry { machine: entry.machine, view: entry.view,
        parent: UNINSTALL_KEY.into(), name: entry.subkey.clone() });
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn inventory_filter() {
        assert!(include("普通应用", "uninstall.exe", false, ""));
        assert!(!include("系统组件", "uninstall.exe", true, ""));
        assert!(!include("子项目", "uninstall.exe", false, "父应用"));
        assert!(!include("无命令", "", false, ""));
        assert!(in_view("", false));
        assert!(!in_view("package", false));
        assert!(!in_view("", true));
        assert!(in_view("package", true));
    }
    #[test] fn command_and_icon() {
        assert_eq!(split_command("\"C:\\Program Files\\Demo\\uninstall.exe\" /quiet").unwrap(),
            ("C:\\Program Files\\Demo\\uninstall.exe".into(), "/quiet".into()));
        assert_eq!(split_command("C:\\Program Files\\Demo\\uninstall.exe /quiet").unwrap().0,
            "C:\\Program Files\\Demo\\uninstall.exe");
        assert_eq!(msi_command("MsiExec.exe /I{12345678}"), "MsiExec.exe /X{12345678}");
        assert_eq!(parse_display_icon("\"C:\\Program Files\\Demo\\app.exe\",-12"),
            ("C:\\Program Files\\Demo\\app.exe".into(), -12));
        assert_eq!(parse_display_icon("C:\\Apps\\Demo, Inc\\app.exe"),
            ("C:\\Apps\\Demo, Inc\\app.exe".into(), 0));
        assert_eq!(store_uninstall_command("Demo'Package"),
            "Remove-AppxPackage -Package 'Demo''Package' -ErrorAction Stop");
        assert_eq!(display_path(Path::new(r"\\?\C:\Demo")), r"C:\Demo");
        assert_eq!(display_path(Path::new(r"\\?\UNC\server\share")), r"\\server\share");
        assert!(!store_family_absent("..\\Other"));
    }
    #[test] fn date_and_sort() {
        assert_eq!(install_date("20261015"), "2026-10-15");
        assert!(install_date("20261340").is_empty());
        assert!(install_date("20260229").is_empty());
        assert_eq!(install_date("20240229"), "2024-02-29");
        assert!(install_date("unknown").is_empty());
        let basic = Entry { name: "B".into(), version: String::new(), publisher: String::new(),
            source: String::new(), command: String::new(), subkey: String::new(),
            package: String::new(), package_family: String::new(),
            display_icon: String::new(), install_location: String::new(),
            install_date: "2026-01-01".into(), machine: false, view: 0, msi: false, image: 0 };
        let newer = Entry { name: "A".into(), install_date: "2026-10-15".into(), ..basic.clone() };
        let unknown = Entry { name: "C".into(), install_date: String::new(), ..basic.clone() };
        assert!(entry_order(&newer, &basic, 4, false).is_gt());
        assert!(entry_order(&newer, &basic, 4, true).is_lt());
        assert!(entry_order(&unknown, &basic, 4, true).is_gt());
    }
    #[test] fn force_remove_rejects_shared_folder() {
        let folder = std::env::current_dir().unwrap().join("target")
            .join(format!("candidate-check-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let entry = Entry { name: "Demo".into(), version: String::new(), publisher: String::new(),
            source: String::new(), command: "uninstall.exe".into(), subkey: "demo".into(),
            package: String::new(), package_family: String::new(), display_icon: String::new(),
            install_location: folder.to_string_lossy().into_owned(), install_date: String::new(), machine: false, view: VIEW_64,
            msi: false, image: 0 };
        assert!(safe_directory(&folder, &entry, &[entry.clone()]).is_some());
        let other = Entry { subkey: "other".into(), ..entry.clone() };
        assert!(safe_directory(&folder, &entry, &[entry.clone(), other]).is_none());
        let icon_only = Entry { subkey: "icon-only".into(), install_location: String::new(),
            display_icon: folder.join("removed.exe").to_string_lossy().into_owned(), ..entry.clone() };
        assert_eq!(PathBuf::from(install_folder(&icon_only).unwrap()), folder.canonicalize().unwrap());
        assert!(safe_directory(&folder, &entry, &[entry.clone(), icon_only]).is_none());
        assert!(registry_display_path(&entry).contains("HKEY_CURRENT_USER"));
        let x86 = Entry { machine: true, view: VIEW_32, ..entry.clone() };
        assert!(registry_display_path(&x86).contains("SOFTWARE\\Wow6432Node\\"));
        let candidates = removal_candidates(&entry, &[entry.clone()]);
        assert!(matches!(candidates.last(), Some(RemovalTarget::Registry { parent, name, .. })
            if parent == UNINSTALL_KEY && name == "demo"));
        std::fs::remove_dir(&folder).unwrap();
        let store_root = std::env::current_dir().unwrap().join("target/store-cleanup-fixture");
        let store_folder = store_root.join("Packages/Demo_publisher");
        std::fs::create_dir_all(&store_folder).unwrap();
        let store = Entry { package: "Demo_1.0_x64__publisher".into(),
            package_family: "Demo_publisher".into(), install_location: String::new(), ..entry.clone() };
        assert_eq!(store_data_directory(&store, &[store.clone()], &store_root),
            Some(store_folder.canonicalize().unwrap()));
        let invalid = Entry { package_family: "..\\Other".into(), ..store.clone() };
        assert!(store_data_directory(&invalid, &[invalid.clone()], &store_root).is_none());
        std::fs::remove_dir_all(&store_root).unwrap();
    }
}

fn message(window: Hwnd, title: &str, text: &str, flags: u32) -> i32 {
    unsafe { MessageBoxW(window, wide(text).as_ptr(), wide(title).as_ptr(), flags) }
}

struct RemovalDialog {
    list: Hwnd,
    count: usize,
    selected: Vec<usize>,
    done: bool,
}

unsafe extern "system" fn removal_dialog_proc(window: Hwnd, message_id: u32, wparam: usize, lparam: isize) -> isize {
    let state = GetWindowLongPtrW(window, -21) as *mut RemovalDialog;
    match message_id {
        WM_COMMAND if !state.is_null() => match wparam & 0xffff {
            ID_REMOVE_CONFIRM => {
                (*state).selected = (0..(*state).count).filter(|index|
                    SendMessageW((*state).list, LVM_GETITEMSTATE, *index, 0xf000) & 0xf000 == 0x2000)
                    .collect();
                DestroyWindow(window);
                return 0;
            }
            ID_REMOVE_CANCEL => { DestroyWindow(window); return 0; }
            _ => {}
        },
        WM_CLOSE => { DestroyWindow(window); return 0; }
        WM_DESTROY if !state.is_null() => { (*state).done = true; return 0; }
        WM_NCDESTROY => { SetWindowLongPtrW(window, -21, 0); }
        _ => {}
    }
    DefWindowProcW(window, message_id, wparam, lparam)
}

fn choose_removals(owner: Hwnd, entry: &Entry, candidates: &[RemovalTarget]) -> Vec<usize> {
    unsafe {
        let instance = GetModuleHandleW(null());
        let class_name = wide("HandyRemovalDialog");
        RegisterClassW(&WndClass { style: 0, proc: Some(removal_dialog_proc), class_extra: 0,
            window_extra: 0, instance, icon: 0, cursor: LoadCursorW(0, 32512usize as *const u16),
            background: 6, menu_name: null(), class_name: class_name.as_ptr() });
        let mut parent: Rect = zeroed();
        GetWindowRect(owner, &mut parent);
        let dialog = CreateWindowExW(1, class_name.as_ptr(), wide("残留检测结果").as_ptr(),
            0x00c80000, parent.left + (parent.right - parent.left - 820) / 2,
            parent.top + (parent.bottom - parent.top - 510) / 2, 820, 510,
            owner, 0, instance, null_mut());
        if dialog == 0 { return Vec::new(); }
        let heading = create_control(instance, dialog, "STATIC",
            &format!("“{}”：发现 {} 个候选项。取消勾选可保留对应项目。", entry.name, candidates.len()),
            WS_CHILD | WS_VISIBLE, 0, 0);
        MoveWindow(heading, 20, 18, 770, 28, 1);
        let list = create_control(instance, dialog, "SysListView32", "",
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | LVS_REPORT | LVS_SHOWSELALWAYS, 0x200, 0);
        MoveWindow(list, 20, 52, 774, 365, 1);
        SendMessageW(list, LVM_SETEXTENDEDLISTVIEWSTYLE, 0x424, 0x424);
        let column_width = candidates.iter().map(|target| target.description().chars().count() as i32 * 10)
            .max().unwrap_or(745).clamp(745, 6000);
        add_column(list, 0, "勾选要删除的文件、快捷方式或注册表项", column_width);
        for (index, candidate) in candidates.iter().enumerate() {
            let mut description = wide(&candidate.description());
            let mut item: LvItem = zeroed();
            item.mask = 1;
            item.item = index as i32;
            item.text = description.as_mut_ptr();
            SendMessageW(list, LVM_INSERTITEMW, 0, &item as *const _ as isize);
            item.state = 0x2000;
            item.state_mask = 0xf000;
            SendMessageW(list, LVM_SETITEMSTATE, index, &item as *const _ as isize);
        }
        let confirm = create_control(instance, dialog, "BUTTON", "清理所选项目",
            WS_CHILD | WS_VISIBLE | WS_TABSTOP, 0, ID_REMOVE_CONFIRM);
        let cancel = create_control(instance, dialog, "BUTTON", "取消",
            WS_CHILD | WS_VISIBLE | WS_TABSTOP, 0, ID_REMOVE_CANCEL);
        MoveWindow(confirm, 600, 428, 116, 32, 1);
        MoveWindow(cancel, 724, 428, 70, 32, 1);
        let mut state = RemovalDialog { list, count: candidates.len(), selected: Vec::new(), done: false };
        SetWindowLongPtrW(dialog, -21, &mut state as *mut _ as isize);
        EnableWindow(owner, 0);
        ShowWindow(dialog, 5);
        UpdateWindow(dialog);
        let mut event: Msg = zeroed();
        while !state.done {
            if GetMessageW(&mut event, 0, 0, 0) <= 0 {
                DestroyWindow(dialog);
                PostQuitMessage(0);
                break;
            }
            TranslateMessage(&event);
            DispatchMessageW(&event);
        }
        EnableWindow(owner, 1);
        SetFocus(owner);
        state.selected
    }
}

struct UninstallWait { result: Option<Result<(), String>> }

unsafe extern "system" fn uninstall_wait_proc(window: Hwnd, message_id: u32, _wparam: usize, lparam: isize) -> isize {
    let state = GetWindowLongPtrW(window, -21) as *mut UninstallWait;
    match message_id {
        WM_APP_DESKTOP_DONE if !state.is_null() => {
            (*state).result = Some(*Box::from_raw(lparam as *mut Result<(), String>));
            DestroyWindow(window);
            return 0;
        }
        WM_CLOSE => return 0,
        WM_NCDESTROY => { SetWindowLongPtrW(window, -21, 0); }
        _ => {}
    }
    DefWindowProcW(window, message_id, _wparam, lparam)
}

fn wait_for_desktop_uninstall(owner: Hwnd, entry: &Entry, file: String, args: String) -> Result<(), String> {
    unsafe {
        let instance = GetModuleHandleW(null());
        let class_name = wide("HandyUninstallWait");
        RegisterClassW(&WndClass { style: 0, proc: Some(uninstall_wait_proc), class_extra: 0,
            window_extra: 0, instance, icon: 0, cursor: LoadCursorW(0, 32512usize as *const u16),
            background: 6, menu_name: null(), class_name: class_name.as_ptr() });
        let mut parent: Rect = zeroed();
        GetWindowRect(owner, &mut parent);
        let dialog = CreateWindowExW(1, class_name.as_ptr(), wide("卸载进行中").as_ptr(),
            0x00c00000, parent.left + (parent.right - parent.left - 520) / 2,
            parent.top + (parent.bottom - parent.top - 150) / 2, 520, 150,
            owner, 0, instance, null_mut());
        if dialog == 0 { return Err("无法创建卸载进度窗口".into()); }
        let label = create_control(instance, dialog, "STATIC",
            &format!("正在卸载“{}”。请完成卸载程序中的操作。", entry.name),
            WS_CHILD | WS_VISIBLE, 0, 0);
        MoveWindow(label, 24, 28, 472, 60, 1);
        let mut state = UninstallWait { result: None };
        SetWindowLongPtrW(dialog, -21, &mut state as *mut _ as isize);
        EnableWindow(owner, 0);
        ShowWindow(dialog, 5);
        UpdateWindow(dialog);
        thread::spawn(move || {
            let parameters = if args.is_empty() { String::new() }
                else { format!(" -ArgumentList {}", ps_quote(&args)) };
            let launch = format!("Start-Process -FilePath {}{} -PassThru -Wait",
                ps_quote(&file), parameters);
            let script = format!("$ErrorActionPreference='Stop'; try {{ try {{ $p={launch} }} catch {{ $e=$_.Exception; while ($e -and $e -isnot [System.ComponentModel.Win32Exception]) {{ $e=$e.InnerException }}; if ($e -and $e.NativeErrorCode -eq 5) {{ $p={launch} -Verb RunAs }} else {{ throw }} }}; if ($p.ExitCode -ne 0 -and $p.ExitCode -ne 3010) {{ throw ('卸载程序退出代码：' + $p.ExitCode) }} }} catch {{ [Console]::Error.WriteLine($_.Exception.Message); exit 1 }}");
            let result = Command::new("powershell.exe")
                .args(["-NoProfile", "-NonInteractive", "-Command", &script])
                .creation_flags(0x08000000).output()
                .map_err(|error| error.to_string())
                .and_then(|output| if output.status.success() { Ok(()) } else {
                    let detail = String::from_utf8_lossy(&output.stderr).trim().chars().take(1000).collect::<String>();
                    Err(if detail.is_empty() { format!("卸载程序退出代码：{:?}", output.status.code()) } else { detail })
                });
            let result = Box::into_raw(Box::new(result));
            if PostMessageW(dialog, WM_APP_DESKTOP_DONE, 0, result as isize) == 0 {
                drop(Box::from_raw(result));
            }
        });
        let mut event: Msg = zeroed();
        while state.result.is_none() {
            if GetMessageW(&mut event, 0, 0, 0) <= 0 {
                DestroyWindow(dialog);
                PostQuitMessage(0);
                break;
            }
            TranslateMessage(&event);
            DispatchMessageW(&event);
        }
        EnableWindow(owner, 1);
        SetFocus(owner);
        state.result.unwrap_or_else(|| Err("卸载等待被中断".into()))
    }
}

fn window_text(window: Hwnd) -> String {
    let length = unsafe { SendMessageW(window, 0x000e, 0, 0) };
    let mut buffer = vec![0; length.max(0) as usize + 1];
    unsafe { SendMessageW(window, 0x000d, buffer.len(), buffer.as_mut_ptr() as isize); }
    from_wide(&buffer)
}

fn create_control(instance: Handle, parent: Hwnd, class: &str, text: &str,
    style: u32, ex_style: u32, id: usize) -> Hwnd {
    unsafe { CreateWindowExW(ex_style, wide(class).as_ptr(), wide(text).as_ptr(), style,
        0, 0, 0, 0, parent, id as Handle, instance, null_mut()) }
}

fn add_column(list: Hwnd, index: usize, title: &str, width: i32) {
    let mut text = wide(title);
    let mut column: LvColumn = unsafe { zeroed() };
    column.mask = 0x0e;
    column.width = width;
    column.text = text.as_mut_ptr();
    column.subitem = index as i32;
    unsafe { SendMessageW(list, LVM_INSERTCOLUMNW, index, &column as *const _ as isize); }
}

fn icon_for(entry: &Entry, gdiplus: Option<&GdiPlus>) -> Handle {
    if !entry.package.is_empty() {
        let Some(gdiplus) = gdiplus else { return 0 };
        if entry.display_icon.is_empty() { return 0; }
        let mut bitmap = null_mut();
        if unsafe { (gdiplus.create_bitmap)(wide(&entry.display_icon).as_ptr(), &mut bitmap) } != 0 {
            return 0;
        }
        let mut icon = 0;
        let status = unsafe { (gdiplus.create_icon)(bitmap, &mut icon) };
        unsafe { (gdiplus.dispose_image)(bitmap); }
        return if status == 0 { icon } else { 0 };
    }
    let mut candidates = vec![parse_display_icon(&entry.display_icon)];
    if let Ok((file, _)) = split_command(&entry.command) { candidates.push((file, 0)); }
    for (path, index) in candidates {
        if !Path::new(&path).is_file() { continue; }
        let (mut large, mut small) = (0, 0);
        let count = unsafe { ExtractIconExW(wide(&path).as_ptr(), index, &mut large, &mut small, 1) };
        let icon = if count != 0 { if large != 0 { large } else { small } } else { 0 };
        unsafe {
            if large != 0 && large != icon { DestroyIcon(large); }
            if small != 0 && small != icon && small != large { DestroyIcon(small); }
        }
        if icon != 0 { return icon; }
    }
    0
}

struct AppState {
    window: Hwnd, search: Hwnd, refresh: Hwnd, monitor: Hwnd, theme: Hwnd,
    desktop_tab: Hwnd, store_tab: Hwnd, list: Hwnd, status: Hwnd, images: Handle,
    title_font: Handle, body_font: Handle, small_font: Handle, bold_font: Handle, stat_font: Handle,
    icon_font: Handle,
    edit_brush: Handle, status_brush: Handle, dark: bool, store_view: bool, search_placeholder: bool,
    entries: Vec<Entry>, visible: Vec<usize>, loading: bool, rendering: bool,
    deleting: bool, uninstalling: bool, sort_column: usize, sort_descending: bool,
    monitor_stop: Option<Arc<AtomicBool>>, monitor_pending: bool,
}

type MonitorUpdate = (Arc<AtomicBool>, Vec<Entry>, Vec<String>, Vec<String>);

impl AppState {
    fn new(window: Hwnd, instance: Handle) -> Self {
        let basic = WS_CHILD | WS_VISIBLE;
        let search = create_control(instance, window, "EDIT", "", basic | WS_TABSTOP | ES_AUTOHSCROLL, 0, ID_SEARCH);
        let monitor = create_control(instance, window, "BUTTON", "开始安装监控", basic | WS_TABSTOP | BS_OWNERDRAW, 0, ID_MONITOR);
        let theme = create_control(instance, window, "BUTTON", "", basic | WS_TABSTOP | BS_OWNERDRAW, 0, ID_THEME);
        let refresh = create_control(instance, window, "BUTTON", "刷新", basic | WS_TABSTOP | BS_OWNERDRAW, 0, ID_REFRESH);
        let desktop_tab = create_control(instance, window, "BUTTON", "桌面应用", basic | WS_TABSTOP | BS_OWNERDRAW, 0, ID_DESKTOP_TAB);
        let store_tab = create_control(instance, window, "BUTTON", "商店应用", basic | WS_TABSTOP | BS_OWNERDRAW, 0, ID_STORE_TAB);
        let list = create_control(instance, window, "SysListView32", "",
            basic | WS_TABSTOP | LVS_REPORT | LVS_SHOWSELALWAYS | LVS_OWNERDRAWFIXED | LVS_NOCOLUMNHEADER, 0, ID_LIST);
        let status = create_control(instance, window, "STATIC", "正在读取应用列表…", basic, 0, 0);
        let title_font = unsafe { CreateFontW(-28, 0, 0, 0, 700, 0, 0, 0, 1, 0, 0, 5, 0,
            wide("Segoe UI").as_ptr()) };
        let body_font = unsafe { CreateFontW(-15, 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 5, 0,
            wide("Segoe UI").as_ptr()) };
        let small_font = unsafe { CreateFontW(-13, 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 5, 0,
            wide("Segoe UI").as_ptr()) };
        let bold_font = unsafe { CreateFontW(-15, 0, 0, 0, 600, 0, 0, 0, 1, 0, 0, 5, 0,
            wide("Segoe UI").as_ptr()) };
        let stat_font = unsafe { CreateFontW(-26, 0, 0, 0, 700, 0, 0, 0, 1, 0, 0, 5, 0,
            wide("Segoe UI").as_ptr()) };
        let icon_font = unsafe { CreateFontW(-19, 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 5, 0,
            wide("Segoe MDL2 Assets").as_ptr()) };
        for (control, font) in [(status, small_font), (search, body_font), (refresh, body_font),
            (monitor, body_font), (theme, body_font), (desktop_tab, bold_font),
            (store_tab, bold_font), (list, body_font)] {
            unsafe { SendMessageW(control, WM_SETFONT, font as usize, 1); }
        }
        unsafe { SetWindowTextW(search, wide("搜索应用名称或发布者").as_ptr()); }
        unsafe { SendMessageW(list, LVM_SETEXTENDEDLISTVIEWSTYLE, 0, 0x10020); }
        add_column(list, 0, "应用名称", 350);
        add_column(list, 1, "版本", 115);
        add_column(list, 2, "发布者", 230);
        add_column(list, 3, "来源", 150);
        add_column(list, 4, "安装时间", 120);
        let images = unsafe { ImageList_Create(24, 24, 0x21, 8, 16) };
        unsafe { SendMessageW(list, LVM_SETIMAGELIST, 1, images); }
        let dark = saved_dark_mode();
        let colors = palette(dark);
        let state = Self { window, search, refresh, monitor, theme, desktop_tab, store_tab, list, status, images,
            title_font, body_font, small_font, bold_font, stat_font, icon_font,
            edit_brush: unsafe { CreateSolidBrush(colors.input) },
            status_brush: unsafe { CreateSolidBrush(colors.chrome) }, dark, store_view: false,
            search_placeholder: true,
            entries: Vec::new(), visible: Vec::new(), loading: false, rendering: false,
            deleting: false, uninstalling: false, sort_column: 0, sort_descending: false,
            monitor_stop: None, monitor_pending: false };
        set_title_dark(window, dark);
        for control in [search, list] { set_control_theme(control, dark); }
        state.color_list();
        unsafe { SetWindowTextW(theme, wide(if dark { "浅色模式" } else { "深色模式" }).as_ptr()); }
        state.layout();
        state
    }

    fn layout(&self) {
        let mut rect: Rect = unsafe { zeroed() };
        unsafe { GetClientRect(self.window, &mut rect); }
        let width = rect.right.max(820);
        let height = rect.bottom.max(540);
        unsafe {
            MoveWindow(self.search, 72, 133, 338, 22, 1);
            MoveWindow(self.monitor, width - 280, 124, 144, 40, 1);
            MoveWindow(self.theme, width - 124, 124, 40, 40, 1);
            MoveWindow(self.refresh, width - 72, 124, 40, 40, 1);
            MoveWindow(self.desktop_tab, 32, 181, 96, 45, 1);
            MoveWindow(self.store_tab, 136, 181, 96, 45, 1);
            MoveWindow(self.list, 32, 264, width - 64, height - 334, 1);
            MoveWindow(self.status, width * 38 / 100, height - 36,
                width * 62 / 100 - 32, 20, 1);
            InvalidateRect(self.window, null(), 0);
        }
        self.fit_columns();
    }

    fn fit_columns(&self) {
        let mut rect: Rect = unsafe { zeroed() };
        unsafe { GetClientRect(self.list, &mut rect); }
        let width = (rect.right - 2).max(0);
        let columns = [width * 35 / 100, width * 14 / 100, width * 24 / 100, width * 14 / 100];
        for (index, column) in columns.iter().enumerate() {
            unsafe { SendMessageW(self.list, LVM_SETCOLUMNWIDTH, index, *column as isize); }
        }
        unsafe { SendMessageW(self.list, LVM_SETCOLUMNWIDTH, 4,
            (width - columns.iter().sum::<i32>()).max(90) as isize); }
    }

    fn static_brush(&self, dc: Handle, control: Hwnd) -> Handle {
        if control != self.status { return 0; }
        let colors = palette(self.dark);
        unsafe { SetBkColor(dc, colors.chrome); SetTextColor(dc, colors.muted); }
        self.status_brush
    }

    fn edit_brush(&self, dc: Handle) -> Handle {
        let colors = palette(self.dark);
        unsafe { SetBkColor(dc, colors.input);
            SetTextColor(dc, if self.search_placeholder { colors.muted } else { colors.text }); }
        self.edit_brush
    }

    fn color_list(&self) {
        let colors = palette(self.dark);
        unsafe {
            SendMessageW(self.list, LVM_SETBKCOLOR, 0, colors.background as isize);
            SendMessageW(self.list, LVM_SETTEXTBKCOLOR, 0, colors.background as isize);
            SendMessageW(self.list, LVM_SETTEXTCOLOR, 0, colors.text as isize);
        }
    }

    fn toggle_theme(&mut self) {
        self.dark = !self.dark;
        let colors = palette(self.dark);
        unsafe {
            DeleteObject(self.edit_brush);
            DeleteObject(self.status_brush);
            self.edit_brush = CreateSolidBrush(colors.input);
            self.status_brush = CreateSolidBrush(colors.chrome);
            SetWindowTextW(self.theme, wide(if self.dark { "浅色模式" } else { "深色模式" }).as_ptr());
            for control in [self.window, self.search, self.monitor, self.refresh,
                self.theme, self.desktop_tab, self.store_tab, self.list, self.status] {
                InvalidateRect(control, null(), 1);
            }
        }
        set_title_dark(self.window, self.dark);
        for control in [self.search, self.list] { set_control_theme(control, self.dark); }
        self.color_list();
        if let Err(code) = save_dark_mode(self.dark) {
            self.set_status(&format!("外观已切换，但无法保存设置（注册表错误代码：{code}）"));
        }
    }

    fn paint_shell(&self) {
        let mut paint: PaintStruct = unsafe { zeroed() };
        let dc = unsafe { BeginPaint(self.window, &mut paint) };
        if dc == 0 { return; }
        let mut client: Rect = unsafe { zeroed() };
        unsafe { GetClientRect(self.window, &mut client); }
        let colors = palette(self.dark);
        let width = client.right;
        let height = client.bottom;
        fill(dc, client, colors.background);
        fill(dc, Rect { left: 0, top: 0, right: width, bottom: 109 }, colors.chrome);
        fill(dc, Rect { left: 0, top: 108, right: width, bottom: 109 }, colors.line);
        draw_text(dc, "管理已安装应用", Rect { left: 32, top: 22, right: 440, bottom: 60 },
            self.title_font, colors.text, 0x20 | 0x4);
        draw_text(dc, "快速查找、卸载应用并清理残留", Rect { left: 33, top: 66, right: 520, bottom: 87 },
            self.small_font, colors.muted, 0x20 | 0x4);
        let total = self.entries.iter().filter(|entry| in_view(&entry.package, self.store_view)).count();
        draw_text(dc, &total.to_string(), Rect { left: width - 185, top: 30, right: width - 118, bottom: 68 },
            self.stat_font, colors.accent, 0x20 | 0x4 | 0x2);
        draw_text(dc, if self.store_view { "个商店应用" } else { "个桌面应用" },
            Rect { left: width - 112, top: 39, right: width - 31, bottom: 62 },
            self.small_font, colors.muted, 0x20 | 0x4);
        rounded(dc, Rect { left: 32, top: 124, right: 424, bottom: 164 },
            colors.input, colors.line, 10);
        unsafe {
            let pen = CreatePen(0, 2, colors.muted);
            let old_pen = SelectObject(dc, pen);
            let old_brush = SelectObject(dc, GetStockObject(5));
            Ellipse(dc, 51, 138, 61, 148);
            MoveToEx(dc, 60, 147, null_mut());
            LineTo(dc, 66, 153);
            SelectObject(dc, old_brush);
            SelectObject(dc, old_pen);
            DeleteObject(pen);
        }
        fill(dc, Rect { left: 32, top: 225, right: width - 32, bottom: 226 }, colors.line);
        let mut start = 32;
        for (index, label) in ["应用名称", "版本", "发布者", "来源", "安装时间"].iter().enumerate() {
            let column_width = unsafe { SendMessageW(self.list, LVM_GETCOLUMNWIDTH, index, 0) as i32 };
            draw_text(dc, label, Rect { left: start + 16, top: 231, right: start + column_width - 8, bottom: 259 },
                self.small_font, colors.muted, 0x20 | 0x4 | 0x8000);
            start += column_width;
        }
        fill(dc, Rect { left: 32, top: 263, right: width - 32, bottom: 264 }, colors.line);
        fill(dc, Rect { left: 0, top: height - 70, right: width, bottom: height }, colors.chrome);
        fill(dc, Rect { left: 0, top: height - 70, right: width, bottom: height - 69 }, colors.line);
        if let Some(index) = self.selected() {
            let entry = &self.entries[index];
            unsafe { ImageList_Draw(self.images, entry.image, dc, 32, height - 48, 1); }
            draw_text(dc, &entry.name, Rect { left: 78, top: height - 62,
                right: width * 38 / 100 - 16, bottom: height - 38 },
                self.bold_font, colors.text, 0x20 | 0x4 | 0x8000);
            draw_text(dc, &format!("已选中应用 · 版本 {}", if entry.version.is_empty() { "未知" } else { &entry.version }),
                Rect { left: 78, top: height - 38, right: width * 38 / 100 - 16,
                    bottom: height - 18 }, self.small_font, colors.muted, 0x20 | 0x4 | 0x8000);
        } else {
            draw_text(dc, "选择应用后右键查看操作", Rect { left: 78, top: height - 62,
                right: width * 38 / 100 - 16, bottom: height - 38 },
                self.bold_font, colors.text, 0x20 | 0x4 | 0x8000);
        }
        unsafe { EndPaint(self.window, &paint); }
    }

    fn draw_button(&self, item: &DrawItem) {
        let colors = palette(self.dark);
        let pressed = item.state & 1 != 0;
        let disabled = item.state & 4 != 0;
        let id = item.id as usize;
        if id == ID_DESKTOP_TAB || id == ID_STORE_TAB {
            let active = (id == ID_STORE_TAB) == self.store_view;
            fill(item.dc, item.rect, colors.background);
            fill(item.dc, Rect { top: item.rect.bottom - 1, ..item.rect }, colors.line);
            let label = if id == ID_STORE_TAB { "商店应用" } else { "桌面应用" };
            draw_text(item.dc, label, Rect { right: item.rect.left + 66, ..item.rect },
                self.small_font, if active { colors.accent } else { colors.muted }, 0x20 | 0x4);
            if active {
                let count = self.entries.iter().filter(|entry|
                    in_view(&entry.package, id == ID_STORE_TAB)).count();
                draw_text(item.dc, &count.to_string(), Rect { left: item.rect.left + 70, ..item.rect },
                    self.small_font, if self.dark { color(139, 188, 249) } else { color(91, 150, 226) },
                    0x20 | 0x4);
                fill(item.dc, Rect { left: item.rect.left, top: item.rect.bottom - 3,
                    right: item.rect.left + 82, bottom: item.rect.bottom }, colors.accent);
            }
            return;
        }
        let primary = id == ID_MONITOR;
        fill(item.dc, item.rect, colors.background);
        let background = if primary { if pressed { color(13, 88, 180) } else { color(25, 112, 218) } }
            else if pressed { colors.selected } else { colors.background };
        rounded(item.dc, Rect { left: item.rect.left + 1, top: item.rect.top + 1,
            right: item.rect.right - 1, bottom: item.rect.bottom - 1 }, background,
            if primary { background } else { colors.line }, 10);
        let ink = if disabled { colors.muted } else if primary { color(255, 255, 255) } else { colors.text };
        if id == ID_THEME || id == ID_REFRESH {
            let symbol = if id == ID_REFRESH { "\u{E895}" }
                else if self.dark { "\u{E706}" } else { "\u{E708}" };
            draw_text(item.dc, symbol, item.rect, self.icon_font, ink, 0x20 | 0x4 | 0x1);
        } else {
            draw_text(item.dc, &window_text(item.window), item.rect, self.bold_font, ink,
                0x20 | 0x4 | 0x1);
        }
    }

    fn draw_row(&self, item: &DrawItem) {
        let Some(&index) = self.visible.get(item.item as usize) else { return; };
        let entry = &self.entries[index];
        let colors = palette(self.dark);
        let selected = item.state & 1 != 0;
        let text_color = if (self.deleting || self.uninstalling) && !selected {
            colors.muted
        } else { colors.text };
        fill(item.dc, item.rect, if selected { colors.selected } else { colors.background });
        fill(item.dc, Rect { left: item.rect.left, top: item.rect.bottom - 1,
            right: item.rect.right, bottom: item.rect.bottom }, colors.line);
        unsafe { ImageList_Draw(self.images, entry.image, item.dc,
            item.rect.left + 16, item.rect.top + (item.rect.bottom - item.rect.top - 24) / 2, 1); }
        let widths: [i32; 5] = std::array::from_fn(|column|
            unsafe { SendMessageW(self.list, LVM_GETCOLUMNWIDTH, column, 0) as i32 });
        draw_text(item.dc, &entry.name, Rect { left: item.rect.left + 52, top: item.rect.top,
            right: item.rect.left + widths[0] - 12, bottom: item.rect.bottom - 1 },
            self.bold_font, text_color, 0x20 | 0x4 | 0x8000);
        let mut start = item.rect.left + widths[0];
        for (column, value) in [&entry.version, &entry.publisher, &entry.source,
            &entry.install_date].iter().enumerate() {
            let right = start + widths[column + 1];
            if column == 2 {
                let badge_width = if entry.package.is_empty() { 110 } else { 150 };
                let badge = Rect { left: start + 10, top: item.rect.top + 9,
                    right: (start + 10 + badge_width).min(right - 8), bottom: item.rect.bottom - 9 };
                rounded(item.dc, badge, colors.badge, colors.badge, 8);
                draw_text(item.dc, value, Rect { left: badge.left + 7, top: badge.top,
                    right: badge.right - 4, bottom: badge.bottom }, self.small_font,
                    colors.muted, 0x20 | 0x4 | 0x8000);
            } else {
                draw_text(item.dc, if column == 3 && value.is_empty() { "—" } else { value },
                    Rect { left: start + 16, top: item.rect.top, right: right - 8,
                        bottom: item.rect.bottom - 1 }, self.body_font, text_color,
                    0x20 | 0x4 | 0x8000);
            }
            start = right;
        }
    }

    fn set_status(&self, text: &str) { unsafe { SetWindowTextW(self.status, wide(text).as_ptr()); } }

    fn search_focus(&mut self, focused: bool) {
        if focused && self.search_placeholder {
            self.search_placeholder = false;
            unsafe { SetWindowTextW(self.search, wide("").as_ptr()); }
        } else if !focused && window_text(self.search).is_empty() {
            self.search_placeholder = true;
            unsafe { SetWindowTextW(self.search, wide("搜索应用名称或发布者").as_ptr()); }
        }
        unsafe { InvalidateRect(self.search, null(), 1); }
    }

    fn sort_at(&mut self, x: i32, y: i32) {
        if self.deleting || self.uninstalling || self.loading { return; }
        let mut client: Rect = unsafe { zeroed() };
        unsafe { GetClientRect(self.window, &mut client); }
        if x < 32 || x >= client.right - 32 || !(226..264).contains(&y) { return; }
        let mut edge = 32;
        for column in 0..5 {
            edge += unsafe { SendMessageW(self.list, LVM_GETCOLUMNWIDTH, column, 0) as i32 };
            if x < edge {
                self.sort_descending = self.sort_column == column && !self.sort_descending;
                self.sort_column = column;
                self.render();
                return;
            }
        }
    }

    fn refresh(&mut self) {
        if self.loading { return; }
        self.monitor_pending = false;
        self.loading = true;
        self.sync_controls();
        self.set_status("正在读取应用列表…");
        let window = self.window;
        thread::spawn(move || {
            let result = Box::into_raw(Box::new(inventory()));
            if unsafe { PostMessageW(window, WM_APP_LOADED, 0, result as isize) } == 0 {
                unsafe { drop(Box::from_raw(result)); }
            }
        });
    }

    fn toggle_monitor(&mut self) {
        if let Some(stop) = self.monitor_stop.take() {
            stop.store(true, Ordering::Relaxed);
            unsafe { SetWindowTextW(self.monitor, wide("开始安装监控").as_ptr()); }
            self.set_status("安装监控已停止");
            return;
        }
        if self.loading { return; }
        let stop = Arc::new(AtomicBool::new(false));
        let token = stop.clone();
        let window = self.window;
        let mut known: HashSet<_> = self.entries.iter().map(entry_identity).collect();
        thread::spawn(move || {
            while !token.load(Ordering::Relaxed) {
                for _ in 0..50 {
                    if token.load(Ordering::Relaxed) { return; }
                    thread::sleep(Duration::from_millis(100));
                }
                let (entries, errors) = inventory();
                if token.load(Ordering::Relaxed) { return; }
                let current: HashSet<_> = entries.iter().map(entry_identity).collect();
                if current != known {
                    let added: Vec<String> = entries.iter().filter(|entry| !known.contains(&entry_identity(entry)))
                        .map(|entry| entry.name.clone()).collect();
                    let update: *mut MonitorUpdate = Box::into_raw(Box::new((token.clone(), entries, errors, added)));
                    if unsafe { PostMessageW(window, WM_APP_MONITOR, 0, update as isize) } == 0 {
                        unsafe { drop(Box::from_raw(update)); }
                        return;
                    }
                }
                known = current;
            }
        });
        self.monitor_stop = Some(stop);
        unsafe { SetWindowTextW(self.monitor, wide("停止安装监控").as_ptr()); }
        self.set_status("安装监控中：每 5 秒检查新登记的应用；关闭窗口后停止。");
    }

    fn loaded(&mut self, mut entries: Vec<Entry>, errors: Vec<String>) {
        let selected: HashSet<_> = self.selected_indices().into_iter()
            .map(|index| entry_identity(&self.entries[index])).collect();
        self.rendering = true;
        unsafe { ImageList_Remove(self.images, -1); }
        let generic = unsafe { LoadIconW(0, 32512usize as *const u16) };
        unsafe { ImageList_ReplaceIcon(self.images, -1, generic); }
        let gdiplus = GdiPlus::start();
        for entry in &mut entries {
            let icon = icon_for(entry, gdiplus.as_ref());
            if icon == 0 { continue; }
            let index = unsafe { ImageList_ReplaceIcon(self.images, -1, icon) };
            unsafe { DestroyIcon(icon); }
            if index >= 0 { entry.image = index; }
        }
        self.entries = entries;
        self.visible.clear();
        self.loading = false;
        self.sync_controls();
        self.rendering = false;
        self.render();
        for (row, &index) in self.visible.iter().enumerate() {
            if selected.contains(&entry_identity(&self.entries[index])) {
                let mut state: LvItem = unsafe { zeroed() };
                state.state = 2;
                state.state_mask = 2;
                unsafe { SendMessageW(self.list, LVM_SETITEMSTATE, row, &state as *const _ as isize); }
            }
        }
        if !errors.is_empty() { message(self.window, "部分来源读取失败", &errors.join("\n"), 0x30); }
        if self.monitor_pending && !self.deleting && !self.uninstalling { self.refresh(); }
    }

    fn selected(&self) -> Option<usize> {
        let row = unsafe { SendMessageW(self.list, LVM_GETNEXTITEM, usize::MAX, 2) };
        if row < 0 { None } else { self.visible.get(row as usize).copied() }
    }

    fn sync_controls(&self) {
        let enabled = i32::from(!self.deleting && !self.uninstalling && !self.loading);
        for control in [self.list, self.search, self.desktop_tab, self.store_tab,
            self.refresh, self.monitor] {
            unsafe { EnableWindow(control, enabled); }
        }
        unsafe { InvalidateRect(self.list, null(), 0); }
    }

    fn selected_indices(&self) -> Vec<usize> {
        let mut rows = Vec::new();
        let mut row = usize::MAX;
        loop {
            let next = unsafe { SendMessageW(self.list, LVM_GETNEXTITEM, row, 2) };
            if next < 0 { break; }
            row = next as usize;
            if let Some(&index) = self.visible.get(row) { rows.push(index); }
        }
        rows
    }

    fn context_menu(&mut self, mut point: Point, keyboard: bool) {
        if self.loading || self.deleting || self.uninstalling { return; }
        if !keyboard {
            let mut hit = LvHitTest { point, flags: 0, item: -1, subitem: 0, group: 0 };
            unsafe { ScreenToClient(self.list, &mut hit.point); }
            let row = unsafe { SendMessageW(self.list, LVM_HITTEST, 0, &mut hit as *mut _ as isize) };
            if row < 0 || self.visible.get(row as usize).is_none() { return; }
            if unsafe { SendMessageW(self.list, LVM_GETITEMSTATE, row as usize, 2) } & 2 == 0 {
                let mut state: LvItem = unsafe { zeroed() };
                state.state_mask = 3;
                unsafe { SendMessageW(self.list, LVM_SETITEMSTATE, usize::MAX, &state as *const _ as isize); }
                state.state = 3;
                unsafe { SendMessageW(self.list, LVM_SETITEMSTATE, row as usize, &state as *const _ as isize); }
            }
        } else {
            unsafe { GetCursorPos(&mut point); }
        }
        let Some(index) = self.selected() else { return };
        let entry = &self.entries[index];
        let selection_count = self.selected_indices().len();
        let registry = entry.package.is_empty();
        let folder = install_folder(entry).is_some();
        let menu = unsafe { CreatePopupMenu() };
        if menu == 0 { return; }
        unsafe {
            let busy = self.deleting || self.uninstalling;
            let single = selection_count == 1;
            AppendMenuW(menu, (busy || !single) as u32, MENU_UNINSTALL as usize, wide("卸载(&U)...").as_ptr());
            if selection_count > 1 {
                AppendMenuW(menu, busy as u32, MENU_BATCH_UNINSTALL as usize,
                    wide(&format!("逐个卸载已选的 {selection_count} 个应用...")).as_ptr());
            }
            AppendMenuW(menu, (busy || !single || !registry) as u32, MENU_FORCE_REMOVE as usize, wide("强制删除(&F)...").as_ptr());
            AppendMenuW(menu, (busy || !single || !registry) as u32, MENU_REMOVE_ENTRY as usize, wide("删除条目(&E)...").as_ptr());
            AppendMenuW(menu, 0x800, 0, null());
            AppendMenuW(menu, (!single || !registry) as u32, MENU_REGISTRY as usize, wide("注册表条目(&R)").as_ptr());
            AppendMenuW(menu, (!single || !folder) as u32, MENU_FOLDER as usize, wide("安装文件夹(&I)").as_ptr());
            let action = TrackPopupMenu(menu, 0x102, point.x, point.y, 0, self.window, null());
            DestroyMenu(menu);
            match action as u32 {
                MENU_UNINSTALL | MENU_BATCH_UNINSTALL | MENU_FORCE_REMOVE | MENU_REMOVE_ENTRY if !busy => {
                    self.deleting = true;
                    self.sync_controls();
                    match action as u32 {
                        MENU_UNINSTALL => self.uninstall_selected(),
                        MENU_BATCH_UNINSTALL => self.batch_uninstall_selected(),
                        MENU_FORCE_REMOVE => self.force_remove(),
                        _ => self.remove_entry(),
                    }
                    self.deleting = false;
                    self.sync_controls();
                    if self.monitor_pending && !self.uninstalling { self.refresh(); }
                }
                MENU_REGISTRY => self.open_registry(),
                MENU_FOLDER => self.open_folder(),
                _ => {}
            }
        }
    }

    fn open_folder(&self) {
        let Some(index) = self.selected() else { return };
        let Some(path) = install_folder(&self.entries[index]) else {
            message(self.window, "安装文件夹不可用", "没有找到此应用的安装文件夹。", 0x30);
            return;
        };
        let result = unsafe { ShellExecuteW(self.window, wide("open").as_ptr(), wide(&path).as_ptr(),
            null(), null(), 1) };
        if result <= 32 { message(self.window, "无法打开安装文件夹", &format!("ShellExecute 错误代码：{result}"), 0x10); }
    }

    fn open_registry(&self) {
        let Some(index) = self.selected() else { return };
        let entry = &self.entries[index];
        if !entry.package.is_empty() { return; }
        let path = format!("{UNINSTALL_KEY}\\{}", entry.subkey);
        if open_key(hive(entry.machine), &path, entry.view, KEY_READ).is_none() {
            message(self.window, "注册表条目不可用", "条目已变化，请刷新列表。", 0x30);
            return;
        }
        let mut handle = 0;
        let status = unsafe { RegCreateKeyExW(hive(false),
            wide("Software\\Microsoft\\Windows\\CurrentVersion\\Applets\\Regedit").as_ptr(),
            0, null_mut(), 0, KEY_WRITE, null(), &mut handle, null_mut()) };
        if status != 0 {
            message(self.window, "无法打开注册表编辑器", &format!("注册表错误代码：{status}"), 0x10);
            return;
        }
        let key = Key(handle);
        let last_key = wide(&registry_display_path(entry));
        let status = unsafe { RegSetValueExW(key.0, wide("LastKey").as_ptr(), 0, REG_SZ,
            last_key.as_ptr() as *const u8, (last_key.len() * 2) as u32) };
        if status != 0 {
            message(self.window, "无法打开注册表编辑器", &format!("注册表错误代码：{status}"), 0x10);
            return;
        }
        let result = unsafe { ShellExecuteW(self.window, wide("open").as_ptr(),
            wide("regedit.exe").as_ptr(), wide("/m").as_ptr(), null(), 1) };
        if result <= 32 { message(self.window, "无法打开注册表编辑器", &format!("ShellExecute 错误代码：{result}"), 0x10); }
    }

    fn remove_entry(&mut self) {
        let Some(index) = self.selected() else { return };
        let entry = self.entries[index].clone();
        if !entry.package.is_empty() { return; }
        if !Self::entry_unchanged(&entry) {
            message(self.window, "条目已变化", "注册表条目内容已变化，请刷新列表。", 0x30);
            return;
        }
        if message(self.window, "确认删除条目", &format!(
            "仅删除“{}”的卸载登记项，不删除程序文件。删除后它将从应用列表消失。\n\n继续吗？",
            entry.name), 0x24) != 6 { return; }
        if !Self::entry_unchanged(&entry) {
            message(self.window, "条目已变化", "注册表条目内容已变化，请刷新列表。", 0x30);
            return;
        }
        let target = RemovalTarget::Registry { machine: entry.machine, view: entry.view,
            parent: UNINSTALL_KEY.into(), name: entry.subkey };
        match target.delete() {
            Ok(()) => self.refresh(),
            Err(error) => { message(self.window, "无法删除条目", &error, 0x10); }
        }
    }

    fn entry_unchanged(entry: &Entry) -> bool {
        if !single_key_component(&entry.subkey) { return false; }
        let path = format!("{UNINSTALL_KEY}\\{}", entry.subkey);
        open_key(hive(entry.machine), &path, entry.view, KEY_READ).is_some_and(|key|
            text_value(key.0, "DisplayName").trim() == entry.name
                && text_value(key.0, "UninstallString").trim() == entry.command)
    }

    fn force_remove(&mut self) {
        let Some(index) = self.selected() else { return };
        let entry = self.entries[index].clone();
        if self.scan_and_remove(&entry, true) { self.refresh(); }
    }

    fn scan_and_remove(&mut self, entry: &Entry, registered: bool) -> bool {
        let store = !entry.package.is_empty();
        if store && !store_family_absent(&entry.package_family) {
            message(self.window, "暂不清理残留", "该包系列仍在系统中，或无法确认卸载结果。", 0x30);
            return false;
        }
        let path = format!("{UNINSTALL_KEY}\\{}", entry.subkey);
        let current = !store && open_key(hive(entry.machine), &path, entry.view, KEY_READ).is_some();
        if !store && ((registered && !Self::entry_unchanged(entry)) || (!registered && current)) {
            message(self.window, "条目已变化", "注册表条目内容已变化，请刷新列表。", 0x30);
            return false;
        }
        let entries = self.entries.clone();
        let mut candidates = removal_candidates(entry, &entries);
        candidates.retain(RemovalTarget::exists);
        if candidates.is_empty() {
            message(self.window, "扫描完成", "未发现可安全归属此应用的残留项。", 0x40);
            return false;
        }
        let selected = choose_removals(self.window, entry, &candidates);
        if selected.is_empty() { return false; }
        candidates = selected.into_iter().map(|index| candidates[index].clone()).collect();
        let prompt = format!("已预览 {} 个候选项。将永久删除所列文件、设置和注册表项，无法恢复。\n\n确认清理“{}”吗？",
            candidates.len(), entry.name);
        if message(self.window, "确认清理", &prompt, 0x24) != 6 { return false; }
        let current = !store && open_key(hive(entry.machine), &path, entry.view, KEY_READ).is_some();
        if (store && !store_family_absent(&entry.package_family))
            || (!store && ((registered && !Self::entry_unchanged(entry)) || (!registered && current))) {
            message(self.window, "条目已变化", "注册表条目内容已变化，请刷新列表。", 0x30);
            return false;
        }
        let fresh = removal_candidates(entry, &entries);
        let mut errors = Vec::new();
        let registration = candidates.last().filter(|target| matches!(target,
            RemovalTarget::Registry { parent, .. } if parent == UNINSTALL_KEY));
        let ordinary = if registration.is_some() { &candidates[..candidates.len() - 1] } else { &candidates[..] };
        for candidate in ordinary {
            if !fresh.contains(candidate) {
                errors.push(format!("{}：候选项已变化", candidate.description()));
                continue;
            }
            if let RemovalTarget::Directory(path) = candidate {
                if safe_directory(path, entry, &entries).as_ref() != Some(path) {
                    errors.push(format!("{}：路径已变化或不安全", candidate.description()));
                    continue;
                }
            }
            if let Err(error) = candidate.delete() { errors.push(format!("{}：{error}", candidate.description())); }
        }
        if errors.is_empty() {
            if let Some(target) = registration {
                if let Err(error) = target.delete() { errors.push(error); }
            }
        }
        if errors.is_empty() {
            message(self.window, "清理完成", "已删除所确认的候选项。", 0x40);
        } else {
            message(self.window, "部分候选项未删除", &errors.join("\n"), 0x30);
        }
        true
    }

    fn render(&mut self) {
        self.rendering = true;
        let selected = self.selected_indices();
        let store_view = self.store_view;
        let query = if self.search_placeholder { String::new() }
            else { window_text(self.search).trim().to_lowercase() };
        unsafe { SendMessageW(self.list, LVM_DELETEALLITEMS, 0, 0); }
        self.visible = self.entries.iter().enumerate().filter(|(_, entry)| {
            in_view(&entry.package, store_view) && (query.is_empty()
                || entry.name.to_lowercase().contains(&query)
                || entry.publisher.to_lowercase().contains(&query)
                || entry.version.to_lowercase().contains(&query))
        }).map(|(index, _)| index).collect();
        self.visible.sort_by(|&a, &b| entry_order(&self.entries[a], &self.entries[b],
            self.sort_column, self.sort_descending));
        for (row, &index) in self.visible.iter().enumerate() {
            let entry = &self.entries[index];
            let row = row as i32;
            let mut name = wide(&entry.name);
            let mut item: LvItem = unsafe { zeroed() };
            item.mask = 3;
            item.item = row;
            item.text = name.as_mut_ptr();
            item.image = entry.image;
            unsafe { SendMessageW(self.list, LVM_INSERTITEMW, 0, &item as *const _ as isize); }
            for (column, text) in [&entry.version, &entry.publisher, &entry.source,
                &entry.install_date].iter().enumerate() {
                let mut value = wide(text);
                item.subitem = column as i32 + 1;
                item.text = value.as_mut_ptr();
                unsafe { SendMessageW(self.list, LVM_SETITEMTEXTW, row as usize, &item as *const _ as isize); }
            }
        }
        for (position, index) in selected.into_iter().enumerate() {
            let Some(row) = self.visible.iter().position(|&item| item == index) else { continue };
            let mut state: LvItem = unsafe { zeroed() };
            state.state = if position == 0 { 3 } else { 2 };
            state.state_mask = 3;
            unsafe { SendMessageW(self.list, LVM_SETITEMSTATE, row, &state as *const _ as isize); }
        }
        self.fit_columns();
        self.rendering = false;
        unsafe {
            for control in [self.window, self.desktop_tab, self.store_tab, self.list] {
                InvalidateRect(control, null(), 0);
            }
        }
        let kind = if store_view { "商店应用" } else { "桌面应用" };
        let column = ["应用名称", "版本", "发布者", "来源", "安装时间"][self.sort_column];
        let direction = if self.sort_descending { "降序" } else { "升序" };
        let busy = if self.uninstalling { "；卸载进行中，不能删除其他应用" } else { "" };
        let monitoring = if self.monitor_stop.is_some() { "；安装监控中" } else { "" };
        self.set_status(&format!("已显示 {} 个{kind}；按{column}{direction}排列{busy}{monitoring}", self.visible.len()));
    }

    fn uninstall_selected(&mut self) {
        let Some(index) = self.selected() else { return };
        let entry = self.entries[index].clone();
        if !entry.package.is_empty() {
            let command = store_uninstall_command(&entry.package);
            if message(self.window, "确认卸载", &format!("应用：{}\n\n将执行：\n{}\n\n继续吗？",
                entry.name, command), 0x24) == 6 { self.start_store_uninstalls(vec![entry]); }
            return;
        }
        if self.uninstall_desktop(&entry) { self.refresh(); }
    }

    fn batch_uninstall_selected(&mut self) {
        let entries: Vec<_> = self.selected_indices().into_iter()
            .map(|index| self.entries[index].clone()).collect();
        if entries.len() < 2 { return; }
        let names = entries.iter().take(12).map(|entry| entry.name.as_str()).collect::<Vec<_>>().join("\n");
        let more = if entries.len() > 12 { format!("\n另有 {} 个应用", entries.len() - 12) } else { String::new() };
        if message(self.window, "确认批量卸载", &format!(
            "将逐个卸载 {} 个应用。前一个流程结束前不会启动下一个。\n\n{names}{more}\n\n继续吗？",
            entries.len()), 0x24) != 6 { return; }
        if entries[0].package.is_empty() {
            let mut started = false;
            for entry in &entries {
                if !self.uninstall_desktop(entry) { break; }
                started = true;
            }
            if started { self.refresh(); }
        } else {
            self.start_store_uninstalls(entries);
        }
    }

    fn start_store_uninstalls(&mut self, entries: Vec<Entry>) {
        self.uninstalling = true;
        self.sync_controls();
        self.set_status(&format!("正在逐个卸载 {} 个商店应用；期间不能删除其他应用。", entries.len()));
        let window = self.window;
        thread::spawn(move || {
            let mut completed = Vec::new();
            let mut error = None;
            for entry in entries {
                let command = store_uninstall_command(&entry.package);
                let result = Command::new("powershell.exe")
                    .args(["-NoProfile", "-NonInteractive", "-Command", &command])
                    .creation_flags(0x08000000).status()
                    .map_err(|cause| format!("{}：{cause}", entry.name))
                    .and_then(|status| if status.success() { Ok(()) } else {
                        Err(format!("{}：PowerShell 退出代码 {:?}", entry.name, status.code()))
                    });
                if let Err(cause) = result {
                    error = Some(cause);
                    break;
                }
                completed.push(entry);
            }
            let result = Box::into_raw(Box::new((completed, error)));
            if unsafe { PostMessageW(window, WM_APP_UNINSTALLED, 0, result as isize) } == 0 {
                unsafe { drop(Box::from_raw(result)); }
            }
        });
    }

    fn uninstall_desktop(&mut self, entry: &Entry) -> bool {
        let path = format!("{UNINSTALL_KEY}\\{}", entry.subkey);
        let current = open_key(hive(entry.machine), &path, entry.view, KEY_READ)
            .map(|key| text_value(key.0, "UninstallString").trim().to_owned()).unwrap_or_default();
        if current != entry.command {
            message(self.window, "卸载信息已变化", &format!("“{}”的卸载命令已变化，请刷新后重试。", entry.name), 0x30);
            return false;
        }
        let command = if entry.msi { msi_command(&current) } else { current };
        if message(self.window, "确认卸载", &format!("应用：{}\n\n将执行：\n{}\n\n继续吗？",
            entry.name, command), 0x24) != 6 { return false; }
        let (file, args) = match split_command(&command) {
            Ok(parts) => parts,
            Err(error) => { message(self.window, "无法启动卸载程序", &error, 0x10); return false; }
        };
        if let Err(error) = wait_for_desktop_uninstall(self.window, entry, file, args) {
            message(self.window, "卸载未完成", &error, 0x30);
            self.refresh();
            return false;
        }
        if open_key(hive(entry.machine), &path, entry.view, KEY_READ).is_some() {
            message(self.window, "卸载未确认完成", "卸载登记项仍然存在，未执行残留删除。", 0x30);
            self.refresh();
            return false;
        }
        self.scan_and_remove(entry, false);
        true
    }
}

fn ps_quote(value: &str) -> String { format!("'{}'", value.replace('\'', "''")) }

fn store_uninstall_command(package: &str) -> String {
    format!("Remove-AppxPackage -Package {} -ErrorAction Stop", ps_quote(package))
}

fn store_family_absent(family: &str) -> bool {
    if !single_key_component(family) { return false; }
    let script = format!("$ErrorActionPreference='Stop'; if (Get-AppxPackage | Where-Object {{ $_.PackageFamilyName -eq {} }}) {{ exit 1 }}",
        ps_quote(family));
    Command::new("powershell.exe").args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(0x08000000).status().is_ok_and(|status| status.success())
}

impl Drop for AppState {
    fn drop(&mut self) {
        if let Some(stop) = &self.monitor_stop { stop.store(true, Ordering::Relaxed); }
        unsafe {
            if self.images != 0 { ImageList_Destroy(self.images); }
            for object in [self.title_font, self.body_font, self.small_font,
                self.bold_font, self.stat_font, self.icon_font, self.edit_brush, self.status_brush] {
                if object != 0 { DeleteObject(object); }
            }
        }
    }
}

unsafe extern "system" fn window_proc(window: Hwnd, message_id: u32, wparam: usize, lparam: isize) -> isize {
    let pointer = GetWindowLongPtrW(window, -21) as *mut AppState;
    match message_id {
        WM_GETMINMAXINFO if lparam != 0 => {
            (*(lparam as *mut MinMaxInfo)).min_track = Point { x: 860, y: 600 };
            return 0;
        }
        WM_SIZE if !pointer.is_null() => { (*pointer).layout(); return 0; }
        WM_ERASEBKGND if !pointer.is_null() => return 1,
        WM_PAINT if !pointer.is_null() => { (*pointer).paint_shell(); return 0; }
        WM_CTLCOLORSTATIC if !pointer.is_null() => {
            let brush = (*pointer).static_brush(wparam as Handle, lparam as Hwnd);
            if brush != 0 { return brush; }
        }
        WM_CTLCOLOREDIT if !pointer.is_null() => return (*pointer).edit_brush(wparam as Handle),
        WM_MEASUREITEM if wparam == ID_LIST && lparam != 0 => {
            (*(lparam as *mut MeasureItem)).height = 43;
            return 1;
        }
        WM_DRAWITEM if !pointer.is_null() && lparam != 0 => {
            let item = &*(lparam as *const DrawItem);
            if item.window == (*pointer).list { (*pointer).draw_row(item); }
            else { (*pointer).draw_button(item); }
            return 1;
        }
        WM_LBUTTONDOWN if !pointer.is_null() => {
            let x = lparam as u32 as u16 as i16 as i32;
            let y = ((lparam as u32 >> 16) as u16 as i16) as i32;
            (*pointer).sort_at(x, y);
            return 0;
        }
        WM_CONTEXTMENU if !pointer.is_null() && wparam as Hwnd == (*pointer).list => {
            let point = Point { x: (lparam as u32 as u16 as i16) as i32,
                y: ((lparam as u32 >> 16) as u16 as i16) as i32 };
            (*pointer).context_menu(point, lparam == -1);
            return 0;
        }
        WM_COMMAND if !pointer.is_null() => {
            let id = wparam & 0xffff;
            let notification = (wparam >> 16) & 0xffff;
            match (id, notification) {
                (ID_SEARCH, 0x300) => (*pointer).render(),
                (ID_SEARCH, 0x100) => (*pointer).search_focus(true),
                (ID_SEARCH, 0x200) => (*pointer).search_focus(false),
                (ID_REFRESH, 0) => (*pointer).refresh(),
                (ID_MONITOR, 0) => (*pointer).toggle_monitor(),
                (ID_THEME, 0) => (*pointer).toggle_theme(),
                (ID_DESKTOP_TAB, 0) | (ID_STORE_TAB, 0) => {
                    (*pointer).store_view = id == ID_STORE_TAB;
                    (*pointer).render();
                }
                _ => return DefWindowProcW(window, message_id, wparam, lparam),
            }
            return 0;
        }
        WM_NOTIFY if !pointer.is_null() => {
            if lparam != 0 && !(*pointer).rendering {
                let header = &*(lparam as *const NmHdr);
                if header.hwnd == (*pointer).list && header.code == LVN_ITEMCHANGED {
                    InvalidateRect(window, null(), 0);
                }
            }
            return 0;
        }
        WM_APP_LOADED => {
            let (entries, errors) = *Box::from_raw(lparam as *mut (Vec<Entry>, Vec<String>));
            if !pointer.is_null() { (*pointer).loaded(entries, errors); }
            return 0;
        }
        WM_APP_UNINSTALLED => {
            let (completed, error) = *Box::from_raw(lparam as *mut (Vec<Entry>, Option<String>));
            if !pointer.is_null() {
                let state = &mut *pointer;
                state.uninstalling = false;
                state.deleting = true;
                for entry in &completed { state.scan_and_remove(entry, false); }
                state.deleting = false;
                state.refresh();
                if let Some(error) = error { message(window, "卸载未完成", &error, 0x30); }
            }
            return 0;
        }
        WM_APP_MONITOR => {
            let (token, entries, errors, added) = *Box::from_raw(lparam as *mut MonitorUpdate);
            if !pointer.is_null() {
                let state = &mut *pointer;
                if state.monitor_stop.as_ref().is_some_and(|active| Arc::ptr_eq(active, &token)) {
                    if state.deleting || state.uninstalling || state.loading {
                        state.monitor_pending = true;
                    } else {
                        state.loaded(entries, errors);
                        if !added.is_empty() {
                            state.set_status(&format!("安装监控：发现 {} 个新应用，包括“{}”。",
                                added.len(), added[0]));
                        }
                    }
                }
            }
            return 0;
        }
        WM_DESTROY => { PostQuitMessage(0); return 0; }
        WM_NCDESTROY if !pointer.is_null() => {
            SetWindowLongPtrW(window, -21, 0);
            drop(Box::from_raw(pointer));
        }
        _ => {}
    }
    DefWindowProcW(window, message_id, wparam, lparam)
}

fn main() {
    unsafe {
        InitCommonControlsEx(&InitCommonControls { size: size_of::<InitCommonControls>() as u32, classes: 9 });
        let instance = GetModuleHandleW(null());
        let class_name = wide("HandyUninstallerWindow");
        let class = WndClass { style: 0, proc: Some(window_proc), class_extra: 0, window_extra: 0,
            instance, icon: LoadIconW(instance, 1usize as *const u16),
            cursor: LoadCursorW(0, 32512usize as *const u16), background: 6,
            menu_name: null(), class_name: class_name.as_ptr() };
        if RegisterClassW(&class) == 0 { return; }
        let window = CreateWindowExW(0, class_name.as_ptr(), wide("Handy Uninstaller").as_ptr(),
            WS_OVERLAPPEDWINDOW | 0x02000000, 100, 100, 1200, 760, 0, 0, instance, null_mut());
        if window == 0 { return; }
        let state = Box::into_raw(Box::new(AppState::new(window, instance)));
        SetWindowLongPtrW(window, -21, state as isize);
        ShowWindow(window, 5);
        UpdateWindow(window);
        SetFocus((*state).list);
        (*state).refresh();
        let mut msg: Msg = zeroed();
        while GetMessageW(&mut msg, 0, 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
