# Local Launcher

[English](README.md) | [简体中文](README.zh-CN.md)

Local Launcher is a small Windows desktop launcher for local folders, executable tools, and URLs. It is built with Rust, Win32 APIs, and `windows-reactor`.

## Features

- Manage shortcuts for folders, programs, external URLs, and intranet URLs.
- Search by name, target, category, tags, account name, and notes.
- Filter by favorite, item type, and category.
- Launch programs with arguments and the program folder as working directory.
- Open a program's containing folder and select the executable.
- Pick folders/programs/import files through native Windows dialogs.
- Store program icons and URL favicons as base64 in `items.json`.
- Import JSON by merging entries instead of replacing local data.
- Tray icon with show/exit actions.
- Settings view for startup behavior, close-to-tray, desktop shortcut, data path, and global hotkey.

## Status

This is a Windows-only prototype. It is useful locally, but the dependency setup is not yet a polished public release layout.

Important: `Cargo.toml` currently depends on a local patched `windows-rs` checkout:

```toml
windows-reactor = { path = "work/windows-rs/crates/libs/reactor" }
windows = { path = "work/windows-rs/crates/libs/windows" }
windows-reactor-setup = { path = "work/windows-rs/crates/libs/reactor-setup" }
```

Before publishing as a standalone GitHub repository, choose one of these:

- Keep a compatible local `windows-rs` checkout under `work/windows-rs`.
- Add the patched `windows-rs` checkout as a submodule or vendored dependency.
- Move to a public fork/revision that includes the local `windows-reactor` icon-path support.

Without that, a fresh clone will not build.

## Build

Requirements:

- Windows
- Rust 2024 toolchain
- A compatible local `windows-rs` checkout at `work/windows-rs`

Commands:

```powershell
cargo build
cargo test
cargo run
```

## Data

Runtime data is stored in `items.json` next to the executable working directory. The file is intentionally ignored by git.

Use `items.example.json` as a starting point:

```powershell
Copy-Item items.example.json items.json
```

The first version stores account names and notes only. It does not store passwords.

## Settings

The settings view is opened from the gear button in the header.

Available settings:

- Launch at Windows login.
- Show main window on startup.
- Close window to tray.
- Enable global hotkey.
- Global hotkey text. The first version supports `Ctrl + Alt + Space`.
- Create desktop shortcut.
- Open data file location.

## Development Notes

- The app uses native Windows APIs for file dialogs, shell opening, tray icon, shortcut creation, and global hotkey registration.
- It should not spawn PowerShell or `cmd.exe` for core Windows interactions.
- Specs and implementation plans live under `docs/superpowers/`.

## License

MIT. See `LICENSE`.
