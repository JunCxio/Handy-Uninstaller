# Handy Uninstaller

Windows x64 桌面程序。使用原生 Win32 窗口，无第三方 Rust 依赖。发布后可直接运行单个 `handy-uninstaller.exe`。

## 构建

安装 Rust 工具链后，在项目目录执行：

```powershell
cargo build --release
```

输出文件：`target/release/handy-uninstaller.exe`。

## 功能范围

- 枚举 HKLM、HKCU 的 64 位和 32 位卸载注册项，以及当前用户的商店应用。
- 显示应用名称、版本、发布者、来源和可取得的程序图标；支持搜索、刷新、启动卸载。
- 启动卸载前显示实际命令，并重新读取传统程序的 `UninstallString`，防止列表过期。
- 右键菜单包含卸载、强制删除、删除条目、注册表条目和安装文件夹。删除条目只删除卸载登记项；注册表条目会在注册表编辑器中定位到对应项。
- 强制删除先逐页显示候选项，再要求最终确认。候选项包括明确的安装目录、指向该目录的快捷方式、名称完全一致的 AppData 目录与 `Software` 注册表键，以及卸载登记项。安装路径不明确或与其他登记应用共用时，不会将该目录列为删除候选项。卸载登记项最后删除；若前面的删除失败，则保留登记项。

商店应用的枚举与卸载、快捷方式目标读取使用 Windows 自带的 PowerShell。商店应用没有传统卸载注册项，右键菜单中不适用的操作会禁用。当前筛选和强制删除规则是基于 Geek 逆向证据的保守实现：要求 `DisplayName` 和 `UninstallString` 非空，排除 `SystemComponent=1` 和带 `ParentDisplayName` 的条目；尚未证明与 Geek 的所有显示条件及残留匹配规则完全一致。强制删除不会扫描 MSI 痕迹，也不会按模糊名称删除其他文件或注册表项。

单元测试：`cargo test`。窗口验证使用 `smoke.py`，该脚本只检查列表、图标、搜索和右键菜单，不启动卸载或删除。
