"""Exercise the Rust window without starting an uninstall."""

import ctypes
import struct
import subprocess
import sys
import time
import zlib
from pathlib import Path


user32 = ctypes.WinDLL("user32", use_last_error=True)
user32.FindWindowW.argtypes = [ctypes.c_wchar_p, ctypes.c_wchar_p]
user32.FindWindowW.restype = ctypes.c_void_p
user32.GetWindowThreadProcessId.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_uint32)]
user32.GetWindowThreadProcessId.restype = ctypes.c_uint32
user32.GetClassNameW.argtypes = [ctypes.c_void_p, ctypes.c_wchar_p, ctypes.c_int]
user32.GetClassNameW.restype = ctypes.c_int
enum_callback = ctypes.WINFUNCTYPE(ctypes.c_bool, ctypes.c_void_p, ctypes.c_ssize_t)
user32.EnumWindows.argtypes = [enum_callback, ctypes.c_ssize_t]
user32.FindWindowExW.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_wchar_p, ctypes.c_wchar_p]
user32.FindWindowExW.restype = ctypes.c_void_p
user32.SendMessageW.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_size_t, ctypes.c_ssize_t]
user32.SendMessageW.restype = ctypes.c_ssize_t
user32.GetWindowTextW.argtypes = [ctypes.c_void_p, ctypes.c_wchar_p, ctypes.c_int]
user32.GetWindowTextW.restype = ctypes.c_int
user32.PostMessageW.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_size_t, ctypes.c_ssize_t]
user32.MoveWindow.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int,
                              ctypes.c_int, ctypes.c_int, ctypes.c_int]
user32.ClientToScreen.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
user32.GetMenuItemCount.argtypes = [ctypes.c_void_p]
user32.GetMenuStringW.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_wchar_p, ctypes.c_int, ctypes.c_uint]


class Rect(ctypes.Structure):
    _fields_ = [("left", ctypes.c_long), ("top", ctypes.c_long),
                ("right", ctypes.c_long), ("bottom", ctypes.c_long)]


user32.GetClientRect.argtypes = [ctypes.c_void_p, ctypes.POINTER(Rect)]


class BitmapInfoHeader(ctypes.Structure):
    _fields_ = [("size", ctypes.c_uint32), ("width", ctypes.c_int32),
                ("height", ctypes.c_int32), ("planes", ctypes.c_uint16),
                ("bits", ctypes.c_uint16), ("compression", ctypes.c_uint32),
                ("image_size", ctypes.c_uint32), ("xppm", ctypes.c_int32),
                ("yppm", ctypes.c_int32), ("colors", ctypes.c_uint32),
                ("important", ctypes.c_uint32)]


def save_window(window, path):
    gdi32 = ctypes.WinDLL("gdi32", use_last_error=True)
    rect = Rect()
    user32.GetWindowRect.argtypes = [ctypes.c_void_p, ctypes.POINTER(Rect)]
    user32.GetWindowRect(window, ctypes.byref(rect))
    user32.SetForegroundWindow.argtypes = [ctypes.c_void_p]
    user32.SetForegroundWindow(window)
    time.sleep(0.3)
    width, height = rect.right - rect.left, rect.bottom - rect.top
    user32.GetDC.argtypes = [ctypes.c_void_p]
    user32.GetDC.restype = ctypes.c_void_p
    user32.ReleaseDC.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
    gdi32.CreateCompatibleDC.argtypes = [ctypes.c_void_p]
    gdi32.CreateCompatibleDC.restype = ctypes.c_void_p
    gdi32.CreateCompatibleBitmap.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int]
    gdi32.CreateCompatibleBitmap.restype = ctypes.c_void_p
    gdi32.SelectObject.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
    gdi32.SelectObject.restype = ctypes.c_void_p
    gdi32.BitBlt.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int,
                           ctypes.c_void_p, ctypes.c_int, ctypes.c_int, ctypes.c_uint]
    gdi32.GetDIBits.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_uint, ctypes.c_uint,
                               ctypes.c_void_p, ctypes.c_void_p, ctypes.c_uint]
    gdi32.DeleteObject.argtypes = [ctypes.c_void_p]
    gdi32.DeleteDC.argtypes = [ctypes.c_void_p]
    user32.PrintWindow.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_uint]
    screen = user32.GetDC(None)
    memory = gdi32.CreateCompatibleDC(screen)
    bitmap = gdi32.CreateCompatibleBitmap(screen, width, height)
    old = gdi32.SelectObject(memory, bitmap)
    assert user32.PrintWindow(window, memory, 0) or gdi32.BitBlt(
        memory, 0, 0, width, height, screen, rect.left, rect.top, 0x00CC0020)
    gdi32.SelectObject(memory, old)
    header = BitmapInfoHeader(40, width, -height, 1, 32, 0, width * height * 4, 0, 0, 0, 0)
    pixels = (ctypes.c_ubyte * (width * height * 4))()
    assert gdi32.GetDIBits(memory, bitmap, 0, height, pixels, ctypes.byref(header), 0) == height
    gdi32.DeleteObject(bitmap)
    gdi32.DeleteDC(memory)
    user32.ReleaseDC(None, screen)
    rgba = bytearray(pixels)
    for i in range(0, len(rgba), 4):
        rgba[i], rgba[i + 2], rgba[i + 3] = rgba[i + 2], rgba[i], 255
    raw = b"".join(b"\0" + rgba[y * width * 4:(y + 1) * width * 4] for y in range(height))
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
    path.write_bytes(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
                     + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))

exe = Path(sys.argv[1]).resolve()
process = subprocess.Popen([str(exe)])
try:
    window = None
    for _ in range(100):
        def match(candidate, _):
            global window
            pid = ctypes.c_uint32()
            user32.GetWindowThreadProcessId(candidate, ctypes.byref(pid))
            class_name = ctypes.create_unicode_buffer(100)
            user32.GetClassNameW(candidate, class_name, len(class_name))
            if pid.value == process.pid and class_name.value == "HandyUninstallerWindow":
                window = candidate
                return False
            return True
        user32.EnumWindows(enum_callback(match), 0)
        if window:
            break
        time.sleep(0.1)
    assert window, "窗口未出现"
    listing = user32.FindWindowExW(window, None, "SysListView32", None)
    search = user32.FindWindowExW(window, None, "Edit", None)
    desktop_tab = user32.FindWindowExW(window, None, "Button", "桌面应用")
    store_tab = user32.FindWindowExW(window, None, "Button", "商店应用")
    theme = user32.FindWindowExW(window, None, "Button", "深色模式") or user32.FindWindowExW(
        window, None, "Button", "浅色模式")
    assert listing and search and desktop_tab and store_tab and theme, "列表、搜索框、页签或主题按钮缺失"
    assert not user32.FindWindowExW(window, None, "Button", "卸载所选应用"), "顶部卸载按钮仍存在"
    for _ in range(300):
        count = user32.SendMessageW(listing, 0x1004, 0, 0)
        if count > 0:
            break
        time.sleep(0.1)
    assert count > 0, "应用列表为空"
    user32.SendMessageW(store_tab, 0x00F5, 0, 0)
    if user32.SendMessageW(listing, 0x1004, 0, 0) > 0:
        save_window(window, Path(__file__).with_name("preview-store.png"))
    user32.SendMessageW(desktop_tab, 0x00F5, 0, 0)
    assert user32.SendMessageW(listing, 0x1004, 0, 0) == count, "切回桌面应用后列表不符"
    status = None
    child = None
    while True:
        child = user32.FindWindowExW(window, child, "Static", None)
        if not child:
            break
        content = ctypes.create_unicode_buffer(200)
        user32.GetWindowTextW(child, content, len(content))
        if content.value.startswith("已显示"):
            status = child
            break
    assert status, "状态栏缺失"
    monitor = user32.FindWindowExW(window, None, "Button", "开始安装监控")
    assert monitor, "安装监控按钮缺失"
    user32.SendMessageW(monitor, 0x00F5, 0, 0)
    monitor_text = ctypes.create_unicode_buffer(100)
    user32.GetWindowTextW(monitor, monitor_text, len(monitor_text))
    assert monitor_text.value == "停止安装监控", "安装监控未启动"
    user32.SendMessageW(monitor, 0x00F5, 0, 0)
    user32.GetWindowTextW(monitor, monitor_text, len(monitor_text))
    assert monitor_text.value == "开始安装监控", "安装监控未停止"
    client = Rect()
    assert user32.GetClientRect(listing, ctypes.byref(client))
    columns_width = sum(user32.SendMessageW(listing, 0x101D, column, 0) for column in range(5))
    assert abs(columns_width - client.right) <= 3, "末列未填满列表"
    user32.SendMessageW(window, 0x0201, 1, 50 | (243 << 16))
    status_text = ctypes.create_unicode_buffer(200)
    user32.GetWindowTextW(status, status_text, len(status_text))
    assert "应用名称降序" in status_text.value, "点击列标题没有切换排序"
    images = user32.SendMessageW(listing, 0x1002, 1, 0)
    assert images, "图标列表未建立"
    selected_row = min(8, count - 1)
    row_point = 100 | ((selected_row * 43 + 20) << 16)
    user32.PostMessageW(listing, 0x0201, 1, row_point)
    user32.PostMessageW(listing, 0x0202, 0, row_point)
    time.sleep(0.1)
    assert user32.SendMessageW(listing, 0x100C, -1, 2) == selected_row, "列表选中状态不符"
    theme_text = ctypes.create_unicode_buffer(100)
    user32.GetWindowTextW(theme, theme_text, len(theme_text))
    initial_theme = theme_text.value
    save_window(window, Path(__file__).with_name(
        "preview-dark.png" if initial_theme == "浅色模式" else "preview.png"))
    user32.SendMessageW(theme, 0x00F5, 0, 0)
    user32.GetWindowTextW(theme, theme_text, len(theme_text))
    assert theme_text.value != initial_theme, "主题切换失败"
    user32.SendMessageW(desktop_tab, 0x00F5, 0, 0)
    save_window(window, Path(__file__).with_name(
        "preview-dark.png" if initial_theme == "深色模式" else "preview.png"))
    user32.SendMessageW(theme, 0x00F5, 0, 0)
    user32.SendMessageW(search, 0x0201, 1, 10 | (10 << 16))
    user32.SendMessageW(search, 0x0202, 0, 10 | (10 << 16))
    for character in "7-Zip":
        user32.SendMessageW(search, 0x0102, ord(character), 0)
    time.sleep(0.1)
    filtered = user32.SendMessageW(listing, 0x1004, 0, 0)
    assert 0 < filtered < count, "搜索没有筛选列表"
    user32.SendMessageW(search, 0x00B1, 0, -1)
    user32.SendMessageW(search, 0x0102, 8, 0)
    time.sleep(0.1)
    assert user32.SendMessageW(listing, 0x1004, 0, 0) == count, "清空搜索后列表未恢复"
    rect = Rect()
    assert user32.GetWindowRect(listing, ctypes.byref(rect))
    point = Rect(rect.left + 30, rect.top + 40, 0, 0)
    packed = (point.left & 0xFFFF) | ((point.top & 0xFFFF) << 16)
    assert user32.PostMessageW(window, 0x007B, listing, packed)
    menu_window = None
    for _ in range(50):
        menu_window = user32.FindWindowW("#32768", None)
        if menu_window:
            break
        time.sleep(0.1)
    assert menu_window, "右键菜单未出现"
    menu = user32.SendMessageW(menu_window, 0x01E1, 0, 0)
    assert menu and user32.GetMenuItemCount(menu) == 6, "右键菜单数量不符"
    labels = []
    for index in range(6):
        buffer = ctypes.create_unicode_buffer(100)
        user32.GetMenuStringW(menu, index, buffer, len(buffer), 0x400)
        labels.append(buffer.value)
    assert all(any(label.startswith(name) for label in labels) for name in
               ("卸载", "强制删除", "删除条目", "注册表条目", "安装文件夹")), labels
    assert not any("谷歌" in label for label in labels), labels
    user32.PostMessageW(menu_window, 0x0100, 27, 0)
    time.sleep(0.1)
    window_rect = Rect()
    assert user32.GetWindowRect(window, ctypes.byref(window_rect))
    assert user32.MoveWindow(window, window_rect.left, window_rect.top, 900, 620, 1)
    save_window(window, Path(__file__).with_name("preview-compact.png"))
    if len(sys.argv) > 2:
        time.sleep(float(sys.argv[2]))
    print(f"窗口正常，桌面应用 {count} 项；页签、搜索和右键菜单通过；未执行卸载或删除。")
finally:
    if window:
        user32.PostMessageW(window, 0x0010, 0, 0)
    try:
        process.wait(timeout=10)
    except subprocess.TimeoutExpired:
        process.terminate()
        process.wait(timeout=10)
