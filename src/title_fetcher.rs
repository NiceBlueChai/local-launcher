//! Fetches HTML titles for URL launcher entries.

#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x08000000;

/// Fetches a web page title with built-in Windows PowerShell.
pub fn fetch_title(url: &str) -> Result<String, String> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("URL 必须以 http:// 或 https:// 开头".to_string());
    }

    let script = r#"
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$ErrorActionPreference = 'Stop'
$response = Invoke-WebRequest -Uri $env:LAUNCHER_URL -UseBasicParsing -TimeoutSec 5
$match = [regex]::Match($response.Content, '<title[^>]*>(.*?)</title>', 'IgnoreCase, Singleline')
if (-not $match.Success) {
    throw '网页没有 title'
}
[System.Net.WebUtility]::HtmlDecode($match.Groups[1].Value.Trim())
"#;

    let mut command = Command::new("powershell.exe");
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let output = command
        .env("LAUNCHER_URL", url)
        .args(["-NoProfile", "-Command", script])
        .output()
        .map_err(|e| format!("获取标题失败: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            "获取标题失败".to_string()
        } else {
            stderr
        });
    }

    parse_title_output(&output.stdout).ok_or_else(|| "网页没有 title".to_string())
}

fn parse_title_output(stdout: &[u8]) -> Option<String> {
    let title = String::from_utf8_lossy(stdout)
        .trim()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    (!title.is_empty()).then_some(title)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_title_output() {
        assert_eq!(
            parse_title_output("  示例   标题\r\n".as_bytes()),
            Some("示例 标题".to_string())
        );
    }

    #[test]
    fn empty_title_output_is_none() {
        assert_eq!(parse_title_output(b"\r\n"), None);
    }
}
