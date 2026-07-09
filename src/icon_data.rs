//! Embedded icon data helpers for launcher items.

use crate::model::{ItemKind, LauncherItem};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::fs;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const TEMP_DIR: &str = "windows-reactor-launcher-runtime-icons";
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IconPayload {
    pub format: String,
    pub data: String,
}

/// Returns a local image URI for an embedded item icon.
pub fn icon_uri(item: &LauncherItem) -> Option<String> {
    let format = item.icon_format.as_deref()?;
    let data = item.icon_data.as_deref()?;
    let path = runtime_icon_path(&item.id, format, data)?;
    if !path.exists() {
        write_icon_file(&path, data).ok()?;
    }
    Some(file_uri(&path))
}

/// Downloads the URL origin favicon and returns base64 data for JSON storage.
pub fn fetch_url_icon(kind: &ItemKind, url: &str) -> Result<IconPayload, String> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("URL 必须以 http:// 或 https:// 开头".to_string());
    }

    let path = runtime_download_path(kind, url);
    ensure_temp_dir()?;
    let icon_path = path_string(&path);
    let script = r#"
$ErrorActionPreference = 'Stop'
$uri = [System.Uri]$env:LAUNCHER_URL
$favicon = $uri.Scheme + '://' + $uri.Authority + '/favicon.ico'
Invoke-WebRequest -Uri $favicon -UseBasicParsing -TimeoutSec 5 -OutFile $env:LAUNCHER_ICON_PATH
"#;

    run_powershell(
        script,
        &[("LAUNCHER_URL", url), ("LAUNCHER_ICON_PATH", &icon_path)],
    )?;
    payload_from_file(&path, "ico")
}

/// Extracts the executable associated icon and returns base64 data for JSON storage.
pub fn extract_program_icon(program_path: &str) -> Result<IconPayload, String> {
    let source = Path::new(program_path);
    if !source.exists() {
        return Err(format!("程序不存在: {program_path}"));
    }

    let path = runtime_download_path(&ItemKind::Program, program_path);
    ensure_temp_dir()?;
    let icon_path = path_string(&path);
    let script = r#"
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$icon = [System.Drawing.Icon]::ExtractAssociatedIcon($env:LAUNCHER_PROGRAM_PATH)
if ($null -eq $icon) {
    throw '程序没有关联图标'
}
$bitmap = $icon.ToBitmap()
$bitmap.Save($env:LAUNCHER_ICON_PATH, [System.Drawing.Imaging.ImageFormat]::Png)
$bitmap.Dispose()
$icon.Dispose()
"#;

    run_powershell(
        script,
        &[
            ("LAUNCHER_PROGRAM_PATH", program_path),
            ("LAUNCHER_ICON_PATH", &icon_path),
        ],
    )?;
    payload_from_file(&path, "png")
}

fn payload_from_file(path: &Path, format: &str) -> Result<IconPayload, String> {
    let bytes = fs::read(path).map_err(|e| format!("读取图标失败: {e}"))?;
    let format = detect_format(&bytes).unwrap_or(format);
    Ok(IconPayload {
        format: format.to_string(),
        data: STANDARD.encode(bytes),
    })
}

fn write_icon_file(path: &Path, data: &str) -> Result<(), String> {
    ensure_temp_dir()?;
    let bytes = STANDARD
        .decode(data)
        .map_err(|e| format!("图标 base64 无效: {e}"))?;
    fs::write(path, bytes).map_err(|e| format!("写入运行时图标失败: {e}"))
}

fn runtime_icon_path(id: &str, format: &str, data: &str) -> Option<PathBuf> {
    let bytes = STANDARD.decode(data).ok()?;
    let extension = detect_format(&bytes).or_else(|| normalize_format(format))?;
    Some(temp_dir().join(format!(
        "{}-{}.{}",
        safe_id(id),
        stable_hash(data),
        extension
    )))
}

fn runtime_download_path(kind: &ItemKind, target: &str) -> PathBuf {
    temp_dir().join(format!(
        "{}-{}.{}",
        kind_prefix(kind),
        stable_hash(target),
        icon_extension(kind)
    ))
}

fn temp_dir() -> PathBuf {
    std::env::temp_dir().join(TEMP_DIR)
}

fn ensure_temp_dir() -> Result<(), String> {
    fs::create_dir_all(temp_dir()).map_err(|e| format!("创建运行时图标目录失败: {e}"))
}

fn kind_prefix(kind: &ItemKind) -> &'static str {
    match kind {
        ItemKind::Folder => "folder",
        ItemKind::Program => "program",
        ItemKind::Url => "url",
        ItemKind::IntranetUrl => "intranet-url",
    }
}

fn icon_extension(kind: &ItemKind) -> &'static str {
    match kind {
        ItemKind::Program | ItemKind::Folder => "png",
        ItemKind::Url | ItemKind::IntranetUrl => "ico",
    }
}

fn normalize_format(format: &str) -> Option<&'static str> {
    match format.trim().to_ascii_lowercase().as_str() {
        "png" => Some("png"),
        "ico" => Some("ico"),
        _ => None,
    }
}

fn detect_format(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) {
        return Some("png");
    }
    if bytes.starts_with(&[0x00, 0x00, 0x01, 0x00]) {
        return Some("ico");
    }
    None
}

fn safe_id(id: &str) -> String {
    id.chars()
        .map(|value| {
            if value.is_ascii_alphanumeric() || value == '-' || value == '_' {
                value
            } else {
                '_'
            }
        })
        .collect()
}

fn stable_hash(value: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn file_uri(path: &Path) -> String {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    let path = windows_display_path(&path).replace('\\', "/");
    if path.starts_with("//") {
        format!("file:{path}")
    } else {
        format!("file:///{path}")
    }
}

fn windows_display_path(path: &Path) -> String {
    let text = path.display().to_string();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    text.strip_prefix(r"\\?\").unwrap_or(&text).to_string()
}

fn path_string(path: &Path) -> String {
    path.display().to_string()
}

fn run_powershell(script: &str, envs: &[(&str, &str)]) -> Result<(), String> {
    let mut command = Command::new("powershell.exe");
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    command.args(["-NoProfile", "-Command", script]);
    for (key, value) in envs {
        command.env(key, value);
    }

    let output = command.output().map_err(|e| format!("图标处理失败: {e}"))?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(if stderr.is_empty() {
        "图标处理失败".to_string()
    } else {
        stderr
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_icon_writes_runtime_uri() {
        let item = LauncherItem {
            id: "bad/id".to_string(),
            name: "工具".to_string(),
            kind: ItemKind::Program,
            target: "C:\\Tools\\app.exe".to_string(),
            arguments: String::new(),
            category: "测试".to_string(),
            tags: Vec::new(),
            username: String::new(),
            favorite: false,
            notes: String::new(),
            icon_format: Some("png".to_string()),
            icon_data: Some(STANDARD.encode(b"icon")),
        };

        let uri = icon_uri(&item).unwrap();

        assert!(uri.starts_with("file:///"));
        assert!(uri.ends_with(".png"));
        assert!(uri.contains("bad_id"));
        assert!(!uri.contains("/?/"));
    }

    #[test]
    fn embedded_icon_uses_data_magic_over_stored_format() {
        let item = LauncherItem {
            id: "url".to_string(),
            name: "站点".to_string(),
            kind: ItemKind::Url,
            target: "https://example.com".to_string(),
            arguments: String::new(),
            category: "测试".to_string(),
            tags: Vec::new(),
            username: String::new(),
            favorite: false,
            notes: String::new(),
            icon_format: Some("ico".to_string()),
            icon_data: Some(STANDARD.encode([0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a])),
        };

        let uri = icon_uri(&item).unwrap();

        assert!(uri.ends_with(".png"));
    }

    #[test]
    fn rejects_unknown_icon_format() {
        assert_eq!(normalize_format("svg"), None);
    }

    #[test]
    fn file_uri_strips_windows_extended_path_prefix() {
        let uri = file_uri(Path::new(r"\\?\C:\Users\tester\icon.png"));

        assert_eq!(uri, "file:///C:/Users/tester/icon.png");
    }

    #[test]
    fn file_uri_formats_unc_paths() {
        let uri = file_uri(Path::new(r"\\?\UNC\server\share\icon.png"));

        assert_eq!(uri, "file://server/share/icon.png");
    }
}
