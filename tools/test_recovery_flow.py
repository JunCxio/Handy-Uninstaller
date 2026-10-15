import ctypes
import os
import pathlib
import shutil
import subprocess
import tempfile
import time
import winreg

ROOT = pathlib.Path(__file__).resolve().parents[1]
EXE = ROOT / "target/release/handy-uninstaller.exe"
PROBE = f"{os.getpid()}-{time.time_ns()}"
NAME = f"Handy Recovery Probe {PROBE}"
KEY = rf"Software\Microsoft\Windows\CurrentVersion\Uninstall\HandyRecoveryProbe{PROBE}"
TEST_LOCAL = pathlib.Path(tempfile.gettempdir()) / f"HandyRecoveryProbe{PROBE}-LocalAppData"
FOLDER = TEST_LOCAL / f"HandyRecoveryProbe{PROBE}"
user32 = ctypes.WinDLL("user32", use_last_error=True)
user32.GetWindowThreadProcessId.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_uint32)]
user32.GetClassNameW.argtypes = [ctypes.c_void_p, ctypes.c_wchar_p, ctypes.c_int]
user32.GetWindowTextW.argtypes = [ctypes.c_void_p, ctypes.c_wchar_p, ctypes.c_int]
callback_type = ctypes.WINFUNCTYPE(ctypes.c_bool, ctypes.c_void_p, ctypes.c_ssize_t)
user32.EnumWindows.argtypes = [callback_type, ctypes.c_ssize_t]
user32.FindWindowExW.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_wchar_p, ctypes.c_wchar_p]
user32.FindWindowExW.restype = ctypes.c_void_p
user32.SendMessageW.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_size_t, ctypes.c_ssize_t]
user32.SendMessageW.restype = ctypes.c_ssize_t
user32.PostMessageW.argtypes = user32.SendMessageW.argtypes
user32.IsWindowEnabled.argtypes = [ctypes.c_void_p]
user32.IsWindowEnabled.restype = ctypes.c_bool
user32.GetWindowRect.argtypes = [ctypes.c_void_p, ctypes.c_void_p]


class Rect(ctypes.Structure):
    _fields_ = [(name, ctypes.c_long) for name in ("left", "top", "right", "bottom")]


def until(callback, label, seconds=50):
    limit = time.monotonic() + seconds
    while time.monotonic() < limit:
        result = callback()
        if result:
            return result
        time.sleep(0.1)
    raise AssertionError(label)


def own_window(class_name, title=None):
    found = None

    def check(hwnd, _):
        nonlocal found
        pid = ctypes.c_uint32()
        user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
        cls = ctypes.create_unicode_buffer(100)
        name = ctypes.create_unicode_buffer(200)
        user32.GetClassNameW(hwnd, cls, len(cls))
        user32.GetWindowTextW(hwnd, name, len(name))
        if pid.value == process.pid and cls.value == class_name and (title is None or name.value == title):
            found = hwnd
            return False
        return True

    user32.EnumWindows(callback_type(check), 0)
    return found


FOLDER.mkdir(parents=True)
(FOLDER / "settings.txt").write_text("keep", encoding="utf-8")
with winreg.CreateKey(winreg.HKEY_CURRENT_USER, KEY) as key:
    winreg.SetValueEx(key, "DisplayName", 0, winreg.REG_SZ, NAME)
    winreg.SetValueEx(key, "InstallLocation", 0, winreg.REG_SZ, str(FOLDER))
    winreg.SetValueEx(key, "UninstallString", 0, winreg.REG_SZ, r"C:\Windows\System32\cmd.exe /c exit 0")

environment = os.environ.copy()
environment["LOCALAPPDATA"] = str(TEST_LOCAL)
process = subprocess.Popen([str(EXE)], env=environment)
try:
    window = until(lambda: own_window("HandyUninstallerWindow"), "main window")
    listing = until(lambda: user32.FindWindowExW(window, None, "SysListView32", None), "list")
    search = until(lambda: user32.FindWindowExW(window, None, "EDIT", None), "search")
    undo = until(lambda: user32.FindWindowExW(window, None, "BUTTON", "撤销清理"), "undo")
    until(lambda: user32.IsWindowEnabled(listing)
        and user32.SendMessageW(listing, 0x1004, 0, 0) > 0, "inventory")
    user32.SendMessageW(search, 0x0201, 1, 10 | (10 << 16))
    user32.SendMessageW(search, 0x0202, 0, 10 | (10 << 16))
    for character in NAME:
        user32.SendMessageW(search, 0x0102, ord(character), 0)
    until(lambda: user32.SendMessageW(listing, 0x1004, 0, 0) == 1, "probe not listed")
    rect = Rect()
    user32.GetWindowRect(listing, ctypes.byref(rect))
    packed = ((rect.left + 20) & 0xffff) | (((rect.top + 20) & 0xffff) << 16)
    user32.PostMessageW(window, 0x007B, listing, packed)
    menu = until(lambda: own_window("#32768"), "menu")
    user32.PostMessageW(menu, 0x0100, 0x28, 0)
    user32.PostMessageW(menu, 0x0100, 0x28, 0)
    user32.PostMessageW(menu, 0x0100, 0x0D, 0)
    picker = until(lambda: own_window("HandyRemovalDialog"), "cleanup preview")
    items = user32.FindWindowExW(picker, None, "SysListView32", None)
    assert user32.SendMessageW(items, 0x1004, 0, 0) >= 2, "missing folder or registry candidate"
    assert user32.SendMessageW(items, 0x101D, 1, 0) >= 200, "missing matching-reason column"
    user32.PostMessageW(picker, 0x0111, 301, 0)
    confirm = until(lambda: own_window("#32770", "确认清理"), "cleanup confirmation")
    user32.PostMessageW(confirm, 0x0111, 6, 0)
    complete = until(lambda: own_window("#32770", "清理完成"), "cleanup result")
    user32.PostMessageW(complete, 0x0010, 0, 0)
    until(lambda: user32.IsWindowEnabled(undo), "undo not enabled")
    assert not FOLDER.exists(), "folder was not isolated"
    try:
        winreg.OpenKey(winreg.HKEY_CURRENT_USER, KEY).Close()
    except FileNotFoundError:
        pass
    else:
        raise AssertionError("registry entry was not removed")
    if os.environ.get("HANDY_TEST_FINALIZE") == "1":
        user32.PostMessageW(window, 0x0010, 0, 0)
        confirm = until(lambda: own_window("#32770", "确认关闭"), "close confirmation")
        user32.PostMessageW(confirm, 0x0111, 6, 0)
        process.wait(timeout=10)
        assert not FOLDER.exists(), "finalized folder reappeared"
        assert not any((path / "settings.txt").exists() for path in
            (TEST_LOCAL / ".Handy-Uninstaller-Recovery").glob("*/0")), "backup was not finalized"
    else:
        user32.PostMessageW(undo, 0x00F5, 0, 0)
        complete = until(lambda: own_window("#32770", "撤销完成"), "undo result")
        user32.PostMessageW(complete, 0x0010, 0, 0)
        until(lambda: FOLDER.exists(), "folder was not restored")
        assert (FOLDER / "settings.txt").read_text(encoding="utf-8") == "keep"
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, KEY) as key:
            assert winreg.QueryValueEx(key, "DisplayName")[0] == NAME
        assert not user32.IsWindowEnabled(undo), "undo stayed enabled"
    log = TEST_LOCAL / "HandyUninstaller/operations.tsv"
    assert NAME in log.read_text(encoding="utf-8"), "operation not recorded"
    print("临时应用清理、候选依据、操作记录与关闭时最终清理均通过" if
        os.environ.get("HANDY_TEST_FINALIZE") == "1" else
        "临时应用清理、候选依据、操作记录与撤销均通过")
finally:
    window = own_window("HandyUninstallerWindow")
    if window:
        user32.PostMessageW(window, 0x0010, 0, 0)
    try:
        process.wait(timeout=10)
    except subprocess.TimeoutExpired:
        process.terminate()
        process.wait(timeout=10)
    try:
        winreg.DeleteKey(winreg.HKEY_CURRENT_USER, KEY)
    except FileNotFoundError:
        pass
    if TEST_LOCAL.exists():
        assert TEST_LOCAL.resolve().parent == pathlib.Path(tempfile.gettempdir()).resolve()
        shutil.rmtree(TEST_LOCAL)
