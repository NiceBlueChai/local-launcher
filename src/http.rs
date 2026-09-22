//! Minimal blocking HTTP GET built on WinHTTP, used instead of spawning PowerShell.

use windows::Win32::errhandlingapi::GetLastError;
use windows::Win32::winhttp::{
    ERROR_WINHTTP_CANNOT_CONNECT, ERROR_WINHTTP_CONNECTION_ERROR, ERROR_WINHTTP_INVALID_URL,
    ERROR_WINHTTP_NAME_NOT_RESOLVED, ERROR_WINHTTP_SECURE_FAILURE, ERROR_WINHTTP_TIMEOUT,
    HINTERNET, INTERNET_PORT, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
    WINHTTP_ACCESS_TYPE_DEFAULT_PROXY, WINHTTP_FLAG_SECURE, WINHTTP_QUERY_CONTENT_TYPE,
    WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE, WinHttpCloseHandle, WinHttpConnect,
    WinHttpOpen, WinHttpOpenRequest, WinHttpQueryDataAvailable, WinHttpQueryHeaders,
    WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetTimeouts,
};
use windows::core::{BOOL, PCWSTR, w};

const USER_AGENT: &str = "LocalLauncher/0.1";
const RESOLVE_TIMEOUT_MS: i32 = 2_000;
const CONNECT_TIMEOUT_MS: i32 = 3_000;
const SEND_TIMEOUT_MS: i32 = 5_000;
const RECEIVE_TIMEOUT_MS: i32 = 5_000;
const READ_CHUNK_BYTES: u32 = 16 * 1024;
const INVALID_URL: &str = "URL 必须以 http:// 或 https:// 开头";

/// A fetched document. Non-2xx responses are returned, not rejected, so callers can decide
/// whether a missing page is an error.
pub struct Response {
    pub bytes: Vec<u8>,
    pub content_type: Option<String>,
    pub status: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Target {
    host: String,
    port: u16,
    object: String,
    secure: bool,
}

impl Target {
    fn origin(&self) -> String {
        let scheme = if self.secure { "https" } else { "http" };
        if self.uses_default_port() {
            format!("{scheme}://{}", self.host)
        } else {
            format!("{scheme}://{}:{}", self.host, self.port)
        }
    }

    fn uses_default_port(&self) -> bool {
        self.port == if self.secure { 443 } else { 80 }
    }
}

/// Parses an absolute http(s) URL into the parts WinHTTP needs.
fn parse_url(url: &str) -> Result<Target, String> {
    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| INVALID_URL.to_string())?;
    let secure = match scheme.to_ascii_lowercase().as_str() {
        "https" => true,
        "http" => false,
        _ => return Err(INVALID_URL.to_string()),
    };
    let rest = rest.split('#').next().unwrap_or(rest);
    let (authority, object) = match rest.find('/') {
        Some(index) => rest.split_at(index),
        None => (rest, "/"),
    };
    let authority = match authority.rsplit_once('@') {
        Some((_, host)) => host,
        None => authority,
    };

    let (host, port) = if let Some(bracketed) = authority.strip_prefix('[') {
        match bracketed.split_once(']') {
            Some((host, suffix)) => (
                format!("[{host}]"),
                suffix.strip_prefix(':').unwrap_or_default().to_string(),
            ),
            None => return Err(format!("{INVALID_URL}（IPv6 地址缺少 ]）")),
        }
    } else {
        match authority.split_once(':') {
            Some((host, port)) if !host.contains(']') && !port.contains(':') => {
                (host.to_string(), port.to_string())
            }
            None => (authority.to_string(), String::new()),
            _ => return Err(INVALID_URL.to_string()),
        }
    };

    if host.is_empty() {
        return Err(INVALID_URL.to_string());
    }
    if !host.is_ascii() {
        return Err("暂不支持非 ASCII 域名".to_string());
    }
    let port = if port.is_empty() {
        if secure { 443 } else { 80 }
    } else {
        port.parse::<u16>()
            .map_err(|_| "URL 端口无效".to_string())?
    };

    Ok(Target {
        host: host.to_ascii_lowercase(),
        port,
        object: object.to_string(),
        secure,
    })
}

/// Downloads at most `limit` bytes of `url`.
pub fn get(url: &str, limit: usize) -> Result<Response, String> {
    let target = parse_url(url)?;
    let host = wide_null(&target.host);
    let object = wide_null(&target.object);
    let agent = wide_null(USER_AGENT);

    unsafe {
        // Handles drop in reverse declaration order, so the request closes before its
        // connection and session.
        let session = Guard(open_session(&agent)?);
        let connect = Guard(handle(
            WinHttpConnect(
                session.0,
                PCWSTR(host.as_ptr()),
                target.port as INTERNET_PORT,
                0,
            ),
            "连接服务器失败",
        )?);
        let request = Guard(handle(
            WinHttpOpenRequest(
                connect.0,
                w!("GET"),
                PCWSTR(object.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                None,
                if target.secure {
                    WINHTTP_FLAG_SECURE as u32
                } else {
                    0
                },
            ),
            "创建请求失败",
        )?);

        let _ = WinHttpSetTimeouts(
            request.0,
            RESOLVE_TIMEOUT_MS,
            CONNECT_TIMEOUT_MS,
            SEND_TIMEOUT_MS,
            RECEIVE_TIMEOUT_MS,
        );
        boolean(
            WinHttpSendRequest(request.0, None, None, 0, 0, 0),
            "发送请求失败",
        )?;
        boolean(
            WinHttpReceiveResponse(request.0, core::ptr::null()),
            "接收响应失败",
        )?;

        let status = query_status(request.0);
        let content_type = query_content_type(request.0);
        let bytes = read_body(request.0, limit)?;

        Ok(Response {
            bytes,
            content_type,
            status,
        })
    }
}

/// Returns the `/favicon.ico` of the same origin, or `None` when the site serves none.
pub fn favicon(url: &str, limit: usize) -> Result<Option<Vec<u8>>, String> {
    let target = parse_url(url)?;

    match get(&format!("{}/favicon.ico", target.origin()), limit) {
        Ok(response) if (200..300).contains(&response.status) && !response.bytes.is_empty() => {
            Ok(Some(response.bytes))
        }
        Ok(_) => Ok(None),
        Err(error) => Err(error),
    }
}

unsafe fn open_session(agent: &[u16]) -> Result<HINTERNET, String> {
    unsafe {
        // `AUTOMATIC_PROXY` needs Win 8.1+; older builds reject it and want the plain default.
        let automatic = WinHttpOpen(
            PCWSTR(agent.as_ptr()),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY as u32,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        );
        let session = if automatic.is_null() {
            WinHttpOpen(
                PCWSTR(agent.as_ptr()),
                WINHTTP_ACCESS_TYPE_DEFAULT_PROXY as u32,
                PCWSTR::null(),
                PCWSTR::null(),
                0,
            )
        } else {
            automatic
        };
        if session.is_null() {
            return Err(winhttp_error("初始化 WinHTTP 失败"));
        }

        Ok(session)
    }
}

/// Reads the body, stopping once `limit` bytes arrived.
fn read_body(request: HINTERNET, limit: usize) -> Result<Vec<u8>, String> {
    let mut body = Vec::new();

    loop {
        let mut available = 0u32;
        boolean(
            unsafe { WinHttpQueryDataAvailable(request, &raw mut available) },
            "读取响应失败",
        )?;
        if available == 0 {
            break;
        }

        let want = available.min(READ_CHUNK_BYTES) as usize;
        let mut chunk = vec![0u8; want];
        let mut read = 0u32;
        boolean(
            unsafe {
                WinHttpReadData(
                    request,
                    chunk.as_mut_ptr().cast(),
                    want as u32,
                    &raw mut read,
                )
            },
            "读取响应失败",
        )?;
        chunk.truncate(read as usize);
        if chunk.is_empty() {
            break;
        }
        body.extend_from_slice(&chunk);
        if body.len() >= limit {
            break;
        }
    }

    Ok(body)
}

fn query_status(request: HINTERNET) -> u32 {
    let mut status = 0u32;
    let mut size = size_of::<u32>() as u32;
    let ok = unsafe {
        WinHttpQueryHeaders(
            request,
            (WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER) as u32,
            PCWSTR::null(),
            Some((&raw mut status).cast()),
            &raw mut size,
            None,
        )
    }
    .as_bool();
    // A missing status header should not discard a body that did arrive.
    if ok { status } else { 200 }
}

fn query_content_type(request: HINTERNET) -> Option<String> {
    let mut buffer = [0u16; 256];
    let mut size = (buffer.len() * size_of::<u16>()) as u32;
    let ok = unsafe {
        WinHttpQueryHeaders(
            request,
            WINHTTP_QUERY_CONTENT_TYPE as u32,
            PCWSTR::null(),
            Some(buffer.as_mut_ptr().cast()),
            &raw mut size,
            None,
        )
    }
    .as_bool();
    if !ok {
        return None;
    }

    let chars = (size as usize / size_of::<u16>()).min(buffer.len());
    Some(String::from_utf16_lossy(&buffer[..chars]))
}

struct Guard(HINTERNET);

impl Drop for Guard {
    fn drop(&mut self) {
        unsafe {
            let _ = WinHttpCloseHandle(self.0);
        }
    }
}

fn handle(value: HINTERNET, action: &str) -> Result<HINTERNET, String> {
    if value.is_null() {
        return Err(winhttp_error(action));
    }

    Ok(value)
}

fn boolean(value: BOOL, action: &str) -> Result<(), String> {
    if value.as_bool() {
        Ok(())
    } else {
        Err(winhttp_error(action))
    }
}

/// Turns the last WinHTTP error into a single-line Chinese message for the status bar.
fn winhttp_error(action: &str) -> String {
    let code = unsafe { GetLastError() };
    let label = match code as i32 {
        ERROR_WINHTTP_TIMEOUT => "请求超时",
        ERROR_WINHTTP_NAME_NOT_RESOLVED => "域名无法解析",
        ERROR_WINHTTP_INVALID_URL => "URL 无效",
        ERROR_WINHTTP_CANNOT_CONNECT => "无法连接服务器",
        ERROR_WINHTTP_CONNECTION_ERROR => "连接被中断",
        ERROR_WINHTTP_SECURE_FAILURE => "TLS 握手失败",
        _ => "WinHTTP 调用失败",
    };

    format!("{action}: {label} (WinHTTP {code})")
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default_ports_and_paths() {
        let secure = parse_url("https://Example.COM/docs/a.html?x=1#frag").unwrap();

        assert_eq!(secure.host, "example.com");
        assert_eq!(secure.port, 443);
        assert_eq!(secure.object, "/docs/a.html?x=1");
        assert!(secure.secure);

        let plain = parse_url("http://127.0.0.1").unwrap();

        assert_eq!(plain.port, 80);
        assert_eq!(plain.object, "/");
        assert!(!plain.secure);
    }

    #[test]
    fn drops_credentials_and_keeps_ipv6_port() {
        let url = parse_url("https://user:pss@[::1]:8443/app").unwrap();

        assert_eq!(url.host, "[::1]");
        assert_eq!(url.port, 8443);
        assert_eq!(url.object, "/app");
    }

    #[test]
    fn origin_omits_default_ports() {
        assert_eq!(
            parse_url("https://a.example").unwrap().origin(),
            "https://a.example"
        );
        assert_eq!(
            parse_url("http://a.example:8080").unwrap().origin(),
            "http://a.example:8080"
        );
    }

    #[test]
    fn rejects_non_http_and_unusable_hosts() {
        assert_eq!(parse_url("example.com").unwrap_err(), INVALID_URL);
        assert_eq!(parse_url("ftp://example.com").unwrap_err(), INVALID_URL);
        assert_eq!(parse_url("https://").unwrap_err(), INVALID_URL);
        assert_eq!(
            parse_url("https://例子.测试").unwrap_err(),
            "暂不支持非 ASCII 域名"
        );
        assert_eq!(
            parse_url("https://example.com:99999").unwrap_err(),
            "URL 端口无效"
        );
        assert_eq!(
            parse_url("https://[::1/app").unwrap_err(),
            format!("{INVALID_URL}（IPv6 地址缺少 ]）")
        );
    }

    #[test]
    fn reports_errors_on_a_single_line() {
        let message = winhttp_error("发送请求失败");

        assert!(message.starts_with("发送请求失败: "));
        assert!(message.contains("(WinHTTP "));
        assert!(!message.contains('\n'));
    }
}
