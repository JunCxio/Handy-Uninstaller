#![cfg_attr(not(test), windows_subsystem = "windows")]

use std::collections::HashSet;
use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::ptr::{null, null_mut};
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
const WS_EX_CLIENTEDGE: u32 = 0x00000200;
const LVS_REPORT: u32 = 1;
const LVS_SINGLESEL: u32 = 4;
const LVS_SHOWSELALWAYS: u32 = 8;
const ES_AUTOHSCROLL: u32 = 0x0080;
const WM_SIZE: u32 = 5;
const WM_DESTROY: u32 = 2;
const WM_NCDESTROY: u32 = 0x0082;
const WM_COMMAND: u32 = 0x0111;
const WM_NOTIFY: u32 = 0x004e;
const WM_CONTEXTMENU: u32 = 0x007b;
const WM_SETFONT: u32 = 0x0030;
const WM_APP_LOADED: u32 = 0x8001;
const LVM_FIRST: u32 = 0x1000;
const LVM_SETIMAGELIST: u32 = LVM_FIRST + 3;
const LVM_DELETEALLITEMS: u32 = LVM_FIRST + 9;
const LVM_GETNEXTITEM: u32 = LVM_FIRST + 12;
const LVM_HITTEST: u32 = LVM_FIRST + 18;
const LVM_SETITEMSTATE: u32 = LVM_FIRST + 43;
const LVM_SETEXTENDEDLISTVIEWSTYLE: u32 = LVM_FIRST + 54;
const LVM_INSERTITEMW: u32 = LVM_FIRST + 77;
const LVM_INSERTCOLUMNW: u32 = LVM_FIRST + 97;
const LVM_SETITEMTEXTW: u32 = LVM_FIRST + 116;
const ID_SEARCH: usize = 101;
const ID_REFRESH: usize = 102;
const ID_UNINSTALL: usize = 103;
const ID_VIEW: usize = 104;
const MENU_UNINSTALL: u32 = 201;
const MENU_FORCE_REMOVE: u32 = 202;
const MENU_REMOVE_ENTRY: u32 = 203;
const MENU_REGISTRY: u32 = 204;
const MENU_FOLDER: u32 = 205;

#[repr(C)]
#[derive(Clone, Copy)]
struct Point { x: i32, y: i32 }
#[repr(C)]
struct Rect { left: i32, top: i32, right: i32, bottom: i32 }
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
    fn MoveWindow(window: Hwnd, x: i32, y: i32, width: i32, height: i32, repaint: i32) -> i32;
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
extern "system" { fn GetStockObject(index: i32) -> Handle; }
#[link(name = "comctl32")]
extern "system" {
    fn InitCommonControlsEx(controls: *const InitCommonControls) -> i32;
    fn ImageList_Create(width: i32, height: i32, flags: u32, initial: i32, grow: i32) -> Handle;
    fn ImageList_ReplaceIcon(list: Handle, index: i32, icon: Handle) -> i32;
    fn ImageList_Remove(list: Handle, index: i32) -> i32;
    fn ImageList_Destroy(list: Handle) -> i32;
}
#[link(name = "shell32")]
extern "system" {
    fn ExtractIconExW(file: *const u16, index: i32, large: *mut Handle, small: *mut Handle, count: u32) -> u32;
    fn ShellExecuteW(window: Hwnd, verb: *const u16, file: *const u16,
        arguments: *const u16, directory: *const u16, show: i32) -> Handle;
}

fn wide(text: &str) -> Vec<u16> { text.encode_utf16().chain(std::iter::once(0)).collect() }
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
    command: String, subkey: String, package: String, display_icon: String, install_location: String,
    machine: bool, view: u32, msi: bool, image: i32,
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
                    command, subkey, package: String::new(),
                    display_icon: text_value(key.0, "DisplayIcon").trim().to_owned(),
                    install_location: text_value(key.0, "InstallLocation").trim().to_owned(),
                    machine, view, msi: is_one(key.0, "WindowsInstaller"), image: 0,
                });
            }
        }
    }
    entries
}

fn store_apps(errors: &mut Vec<String>) -> Vec<Entry> {
    let script = "$ProgressPreference='SilentlyContinue'; [Console]::OutputEncoding=[Text.Encoding]::UTF8; Get-AppxPackage | Where-Object { -not $_.IsFramework -and -not $_.IsResourcePackage -and -not $_.NonRemovable } | ForEach-Object { $_.Name + [char]9 + $_.PackageFullName + [char]9 + $_.Version.ToString() + [char]9 + $_.InstallLocation }";
    let child = Command::new("powershell.exe").args(["-NoProfile", "-NonInteractive", "-Command", script])
        .creation_flags(0x08000000).stdout(Stdio::piped()).stderr(Stdio::null()).spawn();
    let Ok(mut child) = child else { errors.push("商店应用读取失败：无法启动 PowerShell".into()); return Vec::new() };
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() < Duration::from_secs(30) => thread::sleep(Duration::from_millis(100)),
            _ => { let _ = child.kill(); errors.push("商店应用读取超时或失败".into()); break; }
        }
    }
    let Ok(output) = child.wait_with_output() else { return Vec::new() };
    if !output.status.success() {
        errors.push(format!("商店应用读取失败：PowerShell 退出代码 {:?}", output.status.code()));
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout).lines().filter_map(|line| {
        let mut fields = line.split('\t');
        let (name, package, version, location) = (fields.next()?, fields.next()?, fields.next()?, fields.next()?);
        if name.is_empty() || package.is_empty() || fields.next().is_some() { return None; }
        Some(Entry { name: name.into(), version: version.into(), publisher: String::new(),
            source: "商店应用 · 当前用户".into(), command: String::new(), subkey: String::new(),
            package: package.into(), display_icon: String::new(), install_location: location.into(),
            machine: false, view: 0, msi: false, image: 0 })
    }).collect()
}

fn inventory() -> (Vec<Entry>, Vec<String>) {
    let mut errors = Vec::new();
    let mut entries = registry_apps(&mut errors);
    entries.extend(store_apps(&mut errors));
    entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    (entries, errors)
}

fn in_view(package: &str, store_view: bool) -> bool {
    package.is_empty() != store_view
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
    if !Path::new(&icon).is_absolute() || !Path::new(&icon).is_file() { return None; }
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

enum RemovalTarget {
    Directory(PathBuf),
    Shortcut(PathBuf),
    Registry { machine: bool, view: u32, parent: String, name: String },
}

impl RemovalTarget {
    fn description(&self) -> String {
        match self {
            Self::Directory(path) => format!("文件夹：{}", path.display()),
            Self::Shortcut(path) => format!("快捷方式：{}", path.display()),
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
        let location = expand_env(other.install_location.trim().trim_matches('"'));
        if let Ok(other_path) = Path::new(&location).canonicalize() {
            let same_entry = other.machine == entry.machine && other.view == entry.view
                && other.subkey == entry.subkey && other.package == entry.package;
            if !same_entry && other_path.starts_with(&path) { return None; }
        }
    }
    Some(path)
}

fn removal_candidates(entry: &Entry, entries: &[Entry]) -> Vec<RemovalTarget> {
    if !entry.package.is_empty() { return Vec::new(); }
    let mut candidates = Vec::new();
    let location = expand_env(entry.install_location.trim().trim_matches('"'));
    let install = safe_directory(Path::new(&location), entry, entries);
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
        for variable in ["APPDATA", "LOCALAPPDATA"] {
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
    }
    #[test] fn force_remove_rejects_shared_folder() {
        let folder = std::env::current_dir().unwrap().join("target")
            .join(format!("candidate-check-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let entry = Entry { name: "Demo".into(), version: String::new(), publisher: String::new(),
            source: String::new(), command: "uninstall.exe".into(), subkey: "demo".into(),
            package: String::new(), display_icon: String::new(),
            install_location: folder.to_string_lossy().into_owned(), machine: false, view: VIEW_64,
            msi: false, image: 0 };
        assert!(safe_directory(&folder, &entry, &[entry.clone()]).is_some());
        let other = Entry { subkey: "other".into(), ..entry.clone() };
        assert!(safe_directory(&folder, &entry, &[entry.clone(), other]).is_none());
        assert!(registry_display_path(&entry).contains("HKEY_CURRENT_USER"));
        let x86 = Entry { machine: true, view: VIEW_32, ..entry.clone() };
        assert!(registry_display_path(&x86).contains("SOFTWARE\\Wow6432Node\\"));
        let candidates = removal_candidates(&entry, &[entry.clone()]);
        assert!(matches!(candidates.last(), Some(RemovalTarget::Registry { parent, name, .. })
            if parent == UNINSTALL_KEY && name == "demo"));
        std::fs::remove_dir(&folder).unwrap();
    }
}

fn message(window: Hwnd, title: &str, text: &str, flags: u32) -> i32 {
    unsafe { MessageBoxW(window, wide(text).as_ptr(), wide(title).as_ptr(), flags) }
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

fn icon_for(entry: &Entry) -> Handle {
    // shortcut: 商店应用缺少 DisplayIcon 时使用通用图标；需要原生图标时解析包清单。
    let mut candidates = vec![parse_display_icon(&entry.display_icon)];
    if let Ok((file, _)) = split_command(&entry.command) { candidates.push((file, 0)); }
    for (path, index) in candidates {
        if !Path::new(&path).is_file() { continue; }
        let (mut large, mut small) = (0, 0);
        let count = unsafe { ExtractIconExW(wide(&path).as_ptr(), index, &mut large, &mut small, 1) };
        let icon = if count != 0 { if small != 0 { small } else { large } } else { 0 };
        unsafe {
            if large != 0 && large != icon { DestroyIcon(large); }
            if small != 0 && small != icon && small != large { DestroyIcon(small); }
        }
        if icon != 0 { return icon; }
    }
    0
}

struct AppState {
    window: Hwnd, label: Hwnd, search: Hwnd, refresh: Hwnd, view: Hwnd,
    uninstall: Hwnd, list: Hwnd, status: Hwnd, images: Handle,
    entries: Vec<Entry>, visible: Vec<usize>, store_view: bool, loading: bool, rendering: bool,
}

impl AppState {
    fn new(window: Hwnd, instance: Handle) -> Self {
        let basic = WS_CHILD | WS_VISIBLE;
        let label = create_control(instance, window, "STATIC", "搜索应用", basic, 0, 0);
        let search = create_control(instance, window, "EDIT", "", basic | WS_TABSTOP | ES_AUTOHSCROLL,
            WS_EX_CLIENTEDGE, ID_SEARCH);
        let refresh = create_control(instance, window, "BUTTON", "刷新", basic | WS_TABSTOP, 0, ID_REFRESH);
        let view = create_control(instance, window, "BUTTON", "查看商店应用", basic | WS_TABSTOP, 0, ID_VIEW);
        let uninstall = create_control(instance, window, "BUTTON", "卸载所选应用", basic | WS_TABSTOP,
            0, ID_UNINSTALL);
        let list = create_control(instance, window, "SysListView32", "",
            basic | WS_TABSTOP | LVS_REPORT | LVS_SINGLESEL | LVS_SHOWSELALWAYS, WS_EX_CLIENTEDGE, 0);
        let status = create_control(instance, window, "STATIC", "正在读取应用列表…", basic, 0, 0);
        let font = unsafe { GetStockObject(17) };
        for control in [label, search, refresh, view, uninstall, list, status] {
            unsafe { SendMessageW(control, WM_SETFONT, font as usize, 1); }
        }
        unsafe {
            SendMessageW(list, LVM_SETEXTENDEDLISTVIEWSTYLE, 0, 0x10020);
            EnableWindow(uninstall, 0);
        }
        add_column(list, 0, "应用名称", 360);
        add_column(list, 1, "版本", 125);
        add_column(list, 2, "发布者", 225);
        add_column(list, 3, "来源", 230);
        let images = unsafe { ImageList_Create(16, 16, 0x21, 8, 16) };
        unsafe { SendMessageW(list, LVM_SETIMAGELIST, 1, images); }
        let state = Self { window, label, search, refresh, view, uninstall, list, status, images,
            entries: Vec::new(), visible: Vec::new(), store_view: false, loading: false, rendering: false };
        state.layout();
        state
    }

    fn layout(&self) {
        let mut rect: Rect = unsafe { zeroed() };
        unsafe { GetClientRect(self.window, &mut rect); }
        let width = rect.right.max(500);
        let height = rect.bottom.max(300);
        unsafe {
            MoveWindow(self.label, 12, 15, 65, 20, 1);
            MoveWindow(self.search, 80, 10, 290, 26, 1);
            MoveWindow(self.refresh, 385, 9, 70, 28, 1);
            MoveWindow(self.view, 465, 9, 120, 28, 1);
            MoveWindow(self.uninstall, width - 157, 9, 145, 28, 1);
            MoveWindow(self.list, 12, 50, width - 24, height - 80, 1);
            MoveWindow(self.status, 12, height - 25, width - 24, 20, 1);
        }
    }

    fn set_status(&self, text: &str) { unsafe { SetWindowTextW(self.status, wide(text).as_ptr()); } }

    fn refresh(&mut self) {
        if self.loading { return; }
        self.loading = true;
        unsafe { EnableWindow(self.refresh, 0); EnableWindow(self.view, 0); EnableWindow(self.uninstall, 0); }
        self.set_status("正在读取应用列表…");
        let window = self.window;
        thread::spawn(move || {
            let result = Box::into_raw(Box::new(inventory()));
            if unsafe { PostMessageW(window, WM_APP_LOADED, 0, result as isize) } == 0 {
                unsafe { drop(Box::from_raw(result)); }
            }
        });
    }

    fn loaded(&mut self, mut entries: Vec<Entry>, errors: Vec<String>) {
        self.rendering = true;
        unsafe { ImageList_Remove(self.images, -1); }
        let generic = unsafe { LoadIconW(0, 32512usize as *const u16) };
        unsafe { ImageList_ReplaceIcon(self.images, -1, generic); }
        for entry in &mut entries {
            let icon = icon_for(entry);
            if icon == 0 { continue; }
            let index = unsafe { ImageList_ReplaceIcon(self.images, -1, icon) };
            unsafe { DestroyIcon(icon); }
            if index >= 0 { entry.image = index; }
        }
        self.entries = entries;
        self.loading = false;
        unsafe { EnableWindow(self.refresh, 1); EnableWindow(self.view, 1); }
        self.rendering = false;
        self.render();
        if !errors.is_empty() { message(self.window, "部分来源读取失败", &errors.join("\n"), 0x30); }
    }

    fn selected(&self) -> Option<usize> {
        let row = unsafe { SendMessageW(self.list, LVM_GETNEXTITEM, usize::MAX, 2) };
        if row < 0 { None } else { self.visible.get(row as usize).copied() }
    }

    fn update_selection(&self) {
        unsafe { EnableWindow(self.uninstall, (!self.loading && self.selected().is_some()) as i32); }
    }

    fn switch_view(&mut self) {
        if self.loading { return; }
        self.store_view = !self.store_view;
        let label = if self.store_view { "查看桌面应用" } else { "查看商店应用" };
        unsafe { SetWindowTextW(self.view, wide(label).as_ptr()); }
        self.render();
    }

    fn context_menu(&mut self, mut point: Point, keyboard: bool) {
        if self.loading { return; }
        if !keyboard {
            let mut hit = LvHitTest { point, flags: 0, item: -1, subitem: 0, group: 0 };
            unsafe { ScreenToClient(self.list, &mut hit.point); }
            let row = unsafe { SendMessageW(self.list, LVM_HITTEST, 0, &mut hit as *mut _ as isize) };
            if row < 0 || self.visible.get(row as usize).is_none() { return; }
            let mut state: LvItem = unsafe { zeroed() };
            state.state_mask = 3;
            unsafe { SendMessageW(self.list, LVM_SETITEMSTATE, usize::MAX, &state as *const _ as isize); }
            state.state = 3;
            unsafe { SendMessageW(self.list, LVM_SETITEMSTATE, row as usize, &state as *const _ as isize); }
        } else {
            unsafe { GetCursorPos(&mut point); }
        }
        let Some(index) = self.selected() else { return };
        let entry = &self.entries[index];
        let registry = entry.package.is_empty();
        let folder = install_folder(entry).is_some();
        let menu = unsafe { CreatePopupMenu() };
        if menu == 0 { return; }
        unsafe {
            AppendMenuW(menu, 0, MENU_UNINSTALL as usize, wide("卸载(&U)...").as_ptr());
            AppendMenuW(menu, (!registry) as u32, MENU_FORCE_REMOVE as usize, wide("强制删除(&F)...").as_ptr());
            AppendMenuW(menu, (!registry) as u32, MENU_REMOVE_ENTRY as usize, wide("删除条目(&E)...").as_ptr());
            AppendMenuW(menu, 0x800, 0, null());
            AppendMenuW(menu, (!registry) as u32, MENU_REGISTRY as usize, wide("注册表条目(&R)").as_ptr());
            AppendMenuW(menu, (!folder) as u32, MENU_FOLDER as usize, wide("安装文件夹(&I)").as_ptr());
            let action = TrackPopupMenu(menu, 0x102, point.x, point.y, 0, self.window, null());
            DestroyMenu(menu);
            match action as u32 {
                MENU_UNINSTALL => self.uninstall_selected(),
                MENU_FORCE_REMOVE => self.force_remove(),
                MENU_REMOVE_ENTRY => self.remove_entry(),
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
        if !entry.package.is_empty() { return; }
        if !Self::entry_unchanged(&entry) {
            message(self.window, "条目已变化", "注册表条目内容已变化，请刷新列表。", 0x30);
            return;
        }
        let entries = self.entries.clone();
        let candidates = removal_candidates(&entry, &entries);
        for (page, group) in candidates.chunks(8).enumerate() {
            let list = group.iter().map(RemovalTarget::description).collect::<Vec<_>>().join("\n");
            let prompt = format!("应用：{}\n候选项第 {} 页，共 {} 页\n\n{}\n\n请核对路径；取消不会删除任何内容。",
                entry.name, page + 1, candidates.len().div_ceil(8), list);
            if message(self.window, "强制删除预览", &prompt, 0x31) != 1 { return; }
        }
        let prompt = format!("已预览 {} 个候选项。将永久删除所列文件、设置和注册表项，无法恢复。\n\n确认强制删除“{}”吗？",
            candidates.len(), entry.name);
        if message(self.window, "确认强制删除", &prompt, 0x24) != 6 { return; }
        if !Self::entry_unchanged(&entry) {
            message(self.window, "条目已变化", "注册表条目内容已变化，请刷新列表。", 0x30);
            return;
        }
        let mut errors = Vec::new();
        for candidate in &candidates[..candidates.len() - 1] {
            if let RemovalTarget::Directory(path) = candidate {
                if safe_directory(path, &entry, &entries).as_ref() != Some(path) {
                    errors.push(format!("{}：路径已变化或不安全", candidate.description()));
                    continue;
                }
            }
            if let Err(error) = candidate.delete() { errors.push(format!("{}：{error}", candidate.description())); }
        }
        if errors.is_empty() {
            if let Err(error) = candidates.last().unwrap().delete() { errors.push(error); }
        }
        self.refresh();
        if errors.is_empty() {
            message(self.window, "强制删除完成", "已删除所确认的候选项。", 0x40);
        } else {
            message(self.window, "部分候选项未删除", &errors.join("\n"), 0x30);
        }
    }

    fn render(&mut self) {
        self.rendering = true;
        let query = window_text(self.search).trim().to_lowercase();
        unsafe { SendMessageW(self.list, LVM_DELETEALLITEMS, 0, 0); }
        self.visible.clear();
        for (index, entry) in self.entries.iter().enumerate() {
            if !in_view(&entry.package, self.store_view) { continue; }
            if !query.is_empty() && !entry.name.to_lowercase().contains(&query)
                && !entry.publisher.to_lowercase().contains(&query)
                && !entry.version.to_lowercase().contains(&query) { continue; }
            let row = self.visible.len() as i32;
            let mut name = wide(&entry.name);
            let mut item: LvItem = unsafe { zeroed() };
            item.mask = 3;
            item.item = row;
            item.text = name.as_mut_ptr();
            item.image = entry.image;
            unsafe { SendMessageW(self.list, LVM_INSERTITEMW, 0, &item as *const _ as isize); }
            for (column, text) in [&entry.version, &entry.publisher, &entry.source].iter().enumerate() {
                let mut value = wide(text);
                item.subitem = column as i32 + 1;
                item.text = value.as_mut_ptr();
                unsafe { SendMessageW(self.list, LVM_SETITEMTEXTW, row as usize, &item as *const _ as isize); }
            }
            self.visible.push(index);
        }
        self.rendering = false;
        self.update_selection();
        let kind = if self.store_view { "商店应用" } else { "桌面应用" };
        self.set_status(&format!("已显示 {} 个{kind}", self.visible.len()));
    }

    fn uninstall_selected(&mut self) {
        let Some(index) = self.selected() else { return };
        let entry = self.entries[index].clone();
        let command = if !entry.package.is_empty() {
            format!("Remove-AppxPackage -Package '{}'", entry.package.replace('\'', "''"))
        } else {
            let path = format!("{UNINSTALL_KEY}\\{}", entry.subkey);
            let current = open_key(hive(entry.machine), &path, entry.view, KEY_READ)
                .map(|key| text_value(key.0, "UninstallString").trim().to_owned()).unwrap_or_default();
            if current != entry.command {
                message(self.window, "卸载信息已变化", "卸载命令已变化，请刷新后重试。", 0x30);
                return;
            }
            if entry.msi { msi_command(&current) } else { current }
        };
        let prompt = format!("应用：{}\n\n将执行：\n{}\n\n继续吗？", entry.name, command);
        if message(self.window, "确认卸载", &prompt, 0x24) != 6 { return; }
        if !entry.package.is_empty() {
            match Command::new("powershell.exe")
                .args(["-NoProfile", "-NonInteractive", "-Command", &command])
                .creation_flags(0x08000000).spawn() {
                Ok(_) => self.set_status(&format!("已启动 {} 的卸载命令；完成后可刷新列表。", entry.name)),
                Err(error) => { message(self.window, "无法启动卸载程序", &error.to_string(), 0x10); }
            }
            return;
        }
        let (file, args) = match split_command(&command) {
            Ok(parts) => parts,
            Err(error) => { message(self.window, "无法启动卸载程序", &error, 0x10); return; }
        };
        let mut result = unsafe { ShellExecuteW(self.window, wide("open").as_ptr(), wide(&file).as_ptr(),
            wide(&args).as_ptr(), null(), 1) };
        if result == 5 {
            result = unsafe { ShellExecuteW(self.window, wide("runas").as_ptr(), wide(&file).as_ptr(),
                wide(&args).as_ptr(), null(), 1) };
        }
        if result <= 32 {
            message(self.window, "无法启动卸载程序", &format!("ShellExecute 错误代码：{result}"), 0x10);
        } else {
            self.set_status(&format!("已启动 {} 的卸载命令；完成后可刷新列表。", entry.name));
        }
    }
}

impl Drop for AppState {
    fn drop(&mut self) { if self.images != 0 { unsafe { ImageList_Destroy(self.images); } } }
}

unsafe extern "system" fn window_proc(window: Hwnd, message_id: u32, wparam: usize, lparam: isize) -> isize {
    let pointer = GetWindowLongPtrW(window, -21) as *mut AppState;
    match message_id {
        WM_SIZE if !pointer.is_null() => { (*pointer).layout(); return 0; }
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
                (ID_REFRESH, 0) => (*pointer).refresh(),
                (ID_VIEW, 0) => (*pointer).switch_view(),
                (ID_UNINSTALL, 0) => (*pointer).uninstall_selected(),
                _ => return DefWindowProcW(window, message_id, wparam, lparam),
            }
            return 0;
        }
        WM_NOTIFY if !pointer.is_null() => {
            if lparam != 0 && !(*pointer).rendering && (*(lparam as *const NmHdr)).hwnd == (*pointer).list {
                (*pointer).update_selection();
            }
            return 0;
        }
        WM_APP_LOADED => {
            let (entries, errors) = *Box::from_raw(lparam as *mut (Vec<Entry>, Vec<String>));
            if !pointer.is_null() { (*pointer).loaded(entries, errors); }
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
        InitCommonControlsEx(&InitCommonControls { size: size_of::<InitCommonControls>() as u32, classes: 1 });
        let instance = GetModuleHandleW(null());
        let class_name = wide("HandyUninstallerWindow");
        let class = WndClass { style: 0, proc: Some(window_proc), class_extra: 0, window_extra: 0,
            instance, icon: LoadIconW(instance, 1usize as *const u16),
            cursor: LoadCursorW(0, 32512usize as *const u16), background: 6,
            menu_name: null(), class_name: class_name.as_ptr() };
        if RegisterClassW(&class) == 0 { return; }
        let window = CreateWindowExW(0, class_name.as_ptr(), wide("Handy Uninstaller").as_ptr(),
            WS_OVERLAPPEDWINDOW, 100, 100, 1000, 600, 0, 0, instance, null_mut());
        if window == 0 { return; }
        let state = Box::into_raw(Box::new(AppState::new(window, instance)));
        SetWindowLongPtrW(window, -21, state as isize);
        ShowWindow(window, 5);
        UpdateWindow(window);
        (*state).refresh();
        let mut msg: Msg = zeroed();
        while GetMessageW(&mut msg, 0, 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
