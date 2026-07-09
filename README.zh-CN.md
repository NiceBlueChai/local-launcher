# Local Launcher

[English](README.md) | [简体中文](README.zh-CN.md)

Local Launcher 是一个小型 Windows 桌面启动器，用来快速打开本地目录、可执行工具程序和网址。它使用 Rust、Win32 API 和 `windows-reactor` 构建。

## 功能

- 管理目录、程序、外网网址和内网网址快捷入口。
- 按名称、目标、分类、标签、账号名和备注搜索。
- 按收藏、入口类型和分类筛选。
- 启动程序时支持启动参数，并默认使用程序所在目录作为工作目录。
- 打开程序所在文件夹并选中可执行文件。
- 通过原生 Windows 对话框选择目录、程序和导入文件。
- 将程序图标和网站 favicon 以 base64 形式存入 `items.json`。
- 导入 JSON 时合并入口，而不是覆盖本地数据。
- 支持托盘图标、显示窗口和退出操作。
- 提供设置页，支持开机自启、关闭到托盘、桌面快捷方式、数据路径和全局热键。

## 状态

这是一个仅支持 Windows 的原型。它已经适合本地使用，但依赖布局还不是完整的公开发布形态。

注意：`Cargo.toml` 当前依赖一个本地打过补丁的 `windows-rs` 目录：

```toml
windows-reactor = { path = "work/windows-rs/crates/libs/reactor" }
windows = { path = "work/windows-rs/crates/libs/windows" }
windows-reactor-setup = { path = "work/windows-rs/crates/libs/reactor-setup" }
```

作为独立 GitHub 仓库发布前，需要选择一种方式：

- 在仓库内保留兼容的本地 `windows-rs` checkout：`work/windows-rs`。
- 将打过补丁的 `windows-rs` 作为 submodule 或 vendored dependency。
- 切换到包含本地 `windows-reactor` 窗口图标路径支持的公开 fork 或 revision。

否则，重新 clone 后无法直接构建。

## 构建

要求：

- Windows
- Rust 2024 toolchain
- 位于 `work/windows-rs` 的兼容本地 `windows-rs` checkout

命令：

```powershell
cargo build
cargo test
cargo run
```

## 数据

运行时数据存储在当前工作目录旁的 `items.json` 中。该文件会被 git 忽略。

可以用 `items.example.json` 作为初始模板：

```powershell
Copy-Item items.example.json items.json
```

第一版只保存账号名和备注，不保存密码。

## 设置

点击标题栏里的齿轮按钮打开设置页。

当前设置项：

- Windows 登录后启动。
- 启动时显示主窗口。
- 关闭窗口时最小化到托盘。
- 启用全局热键。
- 全局热键文本。第一版支持 `Ctrl + Alt + Space`。
- 创建桌面快捷方式。
- 打开数据文件位置。

## 开发说明

- 文件对话框、Shell 打开、托盘图标、快捷方式创建和全局热键注册都使用原生 Windows API。
- 核心 Windows 交互不应该启动 PowerShell 或 `cmd.exe`。
- 规格和实现计划放在 `docs/superpowers/` 下。

## 许可证

MIT。见 `LICENSE`。
