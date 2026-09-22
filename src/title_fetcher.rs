//! Fetches HTML titles for URL launcher entries.

use crate::http;
use windows::Win32::stringapiset::MultiByteToWideChar;
use windows::core::PWSTR;

const MAX_HTML_BYTES: usize = 256 * 1024;
const CP_UTF8: u32 = 65001;

/// Fetches a web page and returns its `<title>` text.
pub fn fetch_title(url: &str) -> Result<String, String> {
    let response = http::get(url, MAX_HTML_BYTES)?;
    if !(200..300).contains(&response.status) {
        return Err(format!("网页返回状态码 {}", response.status));
    }
    let charset = response
        .content_type
        .as_deref()
        .and_then(charset_of_content_type)
        .or_else(|| charset_of_meta_tag(&response.bytes));

    extract_title(&decode_html(&response.bytes, charset.as_deref()))
        .ok_or_else(|| "网页没有 title".to_string())
}

/// Returns the title text with entities decoded and whitespace collapsed.
fn extract_title(html: &str) -> Option<String> {
    // Lowercasing ASCII keeps byte offsets, so hits can index into the original text.
    let haystack = html.to_ascii_lowercase();
    let open = haystack.find("<title")?;
    let content_start = open + haystack[open..].find('>')? + 1;
    let end = content_start + haystack[content_start..].find("</title")?;
    let title = decode_entities(&html[content_start..end]);
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");

    (!title.is_empty()).then_some(title)
}

/// Decodes the entities that appear in page titles; unknown ones are left as-is.
fn decode_entities(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut rest = text;

    while let Some(start) = rest.find('&') {
        output.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find(';') else {
            output.push('&');
            rest = after;
            continue;
        };
        let entity = &after[..end];
        let decoded = match entity {
            "amp" => Some("&".to_string()),
            "lt" => Some("<".to_string()),
            "gt" => Some(">".to_string()),
            "quot" => Some("\"".to_string()),
            "apos" | "#39" => Some("'".to_string()),
            "nbsp" => Some(" ".to_string()),
            _ => match entity.strip_prefix('#') {
                Some(digits) => parse_code_point(digits).and_then(char::from_u32).map(String::from),
                None => None,
            },
        };
        match decoded {
            Some(text) => {
                output.push_str(&text);
                rest = &after[end + 1..];
            }
            None => {
                output.push('&');
                rest = after;
            }
        }
    }
    output.push_str(rest);

    output
}

/// Reads a decimal or `x`-prefixed hexadecimal character reference.
fn parse_code_point(digits: &str) -> Option<u32> {
    match digits.strip_prefix('x').or_else(|| digits.strip_prefix('X')) {
        Some(hex) => u32::from_str_radix(hex, 16).ok(),
        None => digits.parse::<u32>().ok(),
    }
}

/// Returns the charset declared by a `Content-Type` header value.
fn charset_of_content_type(content_type: &str) -> Option<String> {
    let lowered = content_type.to_ascii_lowercase();
    let (_, charset) = lowered.split_once("charset=")?;

    Some(trim_charset(charset).to_string())
}

/// Falls back to the `<meta>` charset declaration when the header omits one.
fn charset_of_meta_tag(bytes: &[u8]) -> Option<String> {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]).to_ascii_lowercase();
    let (_, charset) = head.rsplit_once("charset=")?;

    Some(trim_charset(charset).to_string())
}

fn trim_charset(value: &str) -> &str {
    value
        .split([';', ' ', '\t', '\r', '\n', '"', '\''])
        .next()
        .unwrap_or_default()
}

/// Maps common web charset labels to Windows code pages.
fn code_page(charset: &str) -> Option<u32> {
    match charset {
        "utf-8" | "utf8" => Some(CP_UTF8),
        "gb2312" | "gbk" | "gb18030" => Some(936),
        "big5" | "big5-hkscs" => Some(950),
        "shift_jis" | "sjis" | "windows-31j" => Some(932),
        "euc-kr" => Some(949),
        "iso-8859-1" | "latin1" | "windows-1252" => Some(1252),
        _ => None,
    }
}

/// Decodes a response body, falling back to UTF-8 when the charset is unknown or unusable.
fn decode_html(bytes: &[u8], charset: Option<&str>) -> String {
    let utf8 = || String::from_utf8_lossy(bytes).into_owned();
    let Some(codepage) = charset.and_then(code_page).filter(|codepage| *codepage != CP_UTF8) else {
        return utf8();
    };
    let count =
        unsafe { MultiByteToWideChar(codepage, 0, bytes.as_ptr().cast(), bytes.len() as i32, None, 0) };
    if count <= 0 {
        return utf8();
    }

    let mut wide = vec![0u16; count as usize];
    let written = unsafe {
        MultiByteToWideChar(
            codepage,
            0,
            bytes.as_ptr().cast(),
            bytes.len() as i32,
            Some(PWSTR(wide.as_mut_ptr())),
            count,
        )
    };
    if written <= 0 {
        return utf8();
    }

    String::from_utf16_lossy(&wide[..written as usize])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_plain_titles() {
        assert_eq!(
            extract_title("<html><head><title>  示例   标题 </title>"),
            Some("示例 标题".to_string())
        );
        assert_eq!(
            extract_title("<HTML><TITLE lang=\"zh\">Doc Title</TITLE></HTML>"),
            Some("Doc Title".to_string())
        );
    }

    #[test]
    fn rejects_missing_and_empty_titles() {
        assert_eq!(extract_title("<html><head><h1>x</h1>"), None);
        assert_eq!(extract_title("<title>   </title>"), None);
        assert_eq!(extract_title("<title without attribute"), None);
    }

    #[test]
    fn decodes_title_entities() {
        assert_eq!(
            extract_title("<title>A &amp; B &lt;C&gt; &#39;q&#39; &#x4E2D;&#26032; &nbsp;x</title>"),
            Some("A & B <C> 'q' 中新 x".to_string())
        );
        assert_eq!(
            extract_title("<title>100% &unknown; &amp</title>"),
            Some("100% &unknown; &amp".to_string())
        );
    }

    #[test]
    fn reads_charset_from_headers_and_meta_tags() {
        assert_eq!(
            charset_of_content_type("text/html; charset=UTF-8"),
            Some("utf-8".to_string())
        );
        assert_eq!(
            charset_of_content_type("text/html; Charset=GBK; foo=1"),
            Some("gbk".to_string())
        );
        assert_eq!(charset_of_content_type("text/html"), None);
        assert_eq!(
            charset_of_meta_tag(b"<meta http-equiv=\"Content-Type\" content=\"text/html;charset=gbk\">"),
            Some("gbk".to_string())
        );
        assert_eq!(code_page("gbk"), Some(936));
        assert_eq!(code_page("utf-8"), Some(CP_UTF8));
        assert_eq!(code_page("utf-16"), None);
    }

    #[test]
    fn decodes_gbk_bodies() {
        let gbk = [0xB1u8, 0xEA, 0xCC, 0xE2];

        assert_eq!(decode_html(&gbk, Some("gbk")), "标题");
        assert_eq!(decode_html("标题".as_bytes(), Some("utf-8")), "标题");
        assert_eq!(decode_html(&gbk, Some("made-up")), String::from_utf8_lossy(&gbk));
    }

    #[test]
    fn rejects_non_http_urls_before_network() {
        assert_eq!(
            fetch_title("example.com").unwrap_err(),
            "URL 必须以 http:// 或 https:// 开头"
        );
    }

    #[test]
    fn title_fetcher_does_not_spawn_shells() {
        let source = include_str!("title_fetcher.rs");
        let command_new = concat!("Command", "::new");
        let powershell = concat!("powershell", ".exe");

        assert!(!source.contains(command_new));
        assert!(!source.contains(powershell));
    }

    #[test]
    #[ignore = "需要外网，手动跑 cargo test -- --ignored"]
    fn fetches_a_real_page_title() {
        let title = fetch_title("https://www.baidu.com/").unwrap();

        assert!(!title.is_empty());
        println!("real title: {title}");
    }
}
