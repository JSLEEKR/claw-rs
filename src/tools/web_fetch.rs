//! Web fetch tool — HTTP GET with HTML text extraction

use super::{PermissionLevel, Tool, ToolContext, ToolError, ToolResult};
use async_trait::async_trait;

/// Tool for fetching web pages and extracting text content
pub struct WebFetchTool;

impl WebFetchTool {
    pub fn new() -> Self {
        Self
    }

    /// Maximum response body size (5 MB)
    const MAX_BODY_BYTES: usize = 5 * 1024 * 1024;

    /// Strip HTML tags and extract text content
    pub fn strip_html(html: &str) -> String {
        let mut result = String::with_capacity(html.len() / 2);
        let mut in_tag = false;
        let mut in_script = false;
        let mut in_style = false;
        let mut last_was_whitespace = false;

        let lower = html.to_lowercase();
        let chars: Vec<char> = html.chars().collect();
        let lower_chars: Vec<char> = lower.chars().collect();

        let mut i = 0;
        while i < chars.len() {
            if in_script {
                // Look for </script>
                if i + 9 <= lower_chars.len() {
                    let slice: String = lower_chars[i..i + 9].iter().collect();
                    if slice == "</script>" {
                        in_script = false;
                        i += 9;
                        continue;
                    }
                }
                i += 1;
                continue;
            }

            if in_style {
                // Look for </style>
                if i + 8 <= lower_chars.len() {
                    let slice: String = lower_chars[i..i + 8].iter().collect();
                    if slice == "</style>" {
                        in_style = false;
                        i += 8;
                        continue;
                    }
                }
                i += 1;
                continue;
            }

            let ch = chars[i];

            if ch == '<' {
                // Check for <script or <style
                if i + 7 <= lower_chars.len() {
                    let slice7: String = lower_chars[i..i + 7].iter().collect();
                    if slice7 == "<script" {
                        in_script = true;
                        in_tag = true;
                        i += 1;
                        continue;
                    }
                    if i + 6 <= lower_chars.len() {
                        let slice6: String = lower_chars[i..i + 6].iter().collect();
                        if slice6 == "<style" {
                            in_style = true;
                            in_tag = true;
                            i += 1;
                            continue;
                        }
                    }
                }
                in_tag = true;
                i += 1;
                continue;
            }

            if ch == '>' && in_tag {
                in_tag = false;
                // Add a space after block-level tags to preserve word boundaries
                if !last_was_whitespace {
                    result.push(' ');
                    last_was_whitespace = true;
                }
                i += 1;
                continue;
            }

            if !in_tag {
                // Decode common HTML entities
                if ch == '&' {
                    if i + 4 <= chars.len() {
                        let ahead: String = chars[i..].iter().take(6).collect();
                        if ahead.starts_with("&amp;") {
                            result.push('&');
                            last_was_whitespace = false;
                            i += 5;
                            continue;
                        } else if ahead.starts_with("&lt;") {
                            result.push('<');
                            last_was_whitespace = false;
                            i += 4;
                            continue;
                        } else if ahead.starts_with("&gt;") {
                            result.push('>');
                            last_was_whitespace = false;
                            i += 4;
                            continue;
                        } else if ahead.starts_with("&quot;") {
                            result.push('"');
                            last_was_whitespace = false;
                            i += 6;
                            continue;
                        } else if ahead.starts_with("&nbsp;") {
                            result.push(' ');
                            last_was_whitespace = true;
                            i += 6;
                            continue;
                        } else if ahead.starts_with("&#39;") || ahead.starts_with("&apos") {
                            // &apos; (6 chars) or &#39; (5 chars)
                            if ahead.starts_with("&apos;") {
                                result.push('\'');
                                last_was_whitespace = false;
                                i += 6;
                                continue;
                            } else if ahead.starts_with("&#39;") {
                                result.push('\'');
                                last_was_whitespace = false;
                                i += 5;
                                continue;
                            }
                        }
                    }
                }

                if ch.is_whitespace() {
                    if !last_was_whitespace {
                        result.push(' ');
                        last_was_whitespace = true;
                    }
                } else {
                    result.push(ch);
                    last_was_whitespace = false;
                }
            }

            i += 1;
        }

        // Clean up: collapse multiple newlines, trim
        let trimmed = result.trim().to_string();

        // Limit output length (truncate at a char boundary, not byte boundary)
        if trimmed.len() > 50_000 {
            // Find a valid UTF-8 boundary at or before 50_000 bytes
            let mut end = 50_000;
            while end > 0 && !trimmed.is_char_boundary(end) {
                end -= 1;
            }
            format!("{}...\n\n(truncated at {} bytes)", &trimmed[..end], end)
        } else {
            trimmed
        }
    }
}

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }

    fn description(&self) -> &str {
        "Fetch a URL and extract text content. Supports HTTP/HTTPS. Returns extracted text from HTML pages."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to fetch"
                },
                "raw": {
                    "type": "boolean",
                    "description": "If true, return raw HTML instead of extracted text"
                }
            },
            "required": ["url"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let url = params["url"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("Missing 'url' parameter".into()))?;

        let raw = params["raw"].as_bool().unwrap_or(false);

        // Validate URL
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(ToolError::InvalidParams(
                "URL must start with http:// or https://".into(),
            ));
        }

        // SSRF protection: block private/internal addresses
        if let Ok(parsed) = url::Url::parse(url) {
            let host = parsed.host_str().unwrap_or("");
            let host_lower = host.to_lowercase();

            // Block localhost and loopback variants
            if host_lower == "localhost"
                || host_lower == "127.0.0.1"
                || host_lower == "[::1]"
                || host_lower == "::1"
                || host_lower == "0.0.0.0"
            {
                return Err(ToolError::InvalidParams(
                    "URL targets a loopback address (SSRF protection)".into(),
                ));
            }

            // Block cloud metadata endpoints
            if host_lower == "169.254.169.254"
                || host_lower == "metadata.google.internal"
            {
                return Err(ToolError::InvalidParams(
                    "URL targets a cloud metadata endpoint (SSRF protection)".into(),
                ));
            }

            // Block private IP ranges (10.x, 172.16-31.x, 192.168.x)
            if let Ok(ip) = host.parse::<std::net::IpAddr>() {
                let is_private = match ip {
                    std::net::IpAddr::V4(v4) => {
                        v4.is_loopback()
                            || v4.is_private()
                            || v4.is_link_local()
                            || v4.is_broadcast()
                            || v4.is_unspecified()
                    }
                    std::net::IpAddr::V6(v6) => {
                        v6.is_loopback() || v6.is_unspecified()
                    }
                };
                if is_private {
                    return Err(ToolError::InvalidParams(
                        "URL targets a private/internal IP address (SSRF protection)".into(),
                    ));
                }
            }
        }

        // Build client with timeout
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("claw-rs/1.0")
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to create HTTP client: {}", e)))?;

        let response = client
            .get(url)
            .send()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("HTTP request failed: {}", e)))?;

        let status = response.status();
        if !status.is_success() {
            return Ok(ToolResult::error(format!(
                "HTTP {} {}",
                status.as_u16(),
                status.canonical_reason().unwrap_or("Unknown")
            )));
        }

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        // Check content-length header before downloading (early reject)
        if let Some(content_length) = response.content_length() {
            if content_length > Self::MAX_BODY_BYTES as u64 {
                return Ok(ToolResult::error(format!(
                    "Response too large: {} bytes (max {})",
                    content_length,
                    Self::MAX_BODY_BYTES
                )));
            }
        }

        // Read body with size limit (streaming to avoid OOM on large responses)
        let mut bytes = Vec::new();
        let mut stream = response;
        while let Some(chunk) = stream.chunk().await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to read response body: {}", e)))? {
            bytes.extend_from_slice(&chunk);
            if bytes.len() > Self::MAX_BODY_BYTES {
                return Ok(ToolResult::error(format!(
                    "Response too large: exceeded {} byte limit during download",
                    Self::MAX_BODY_BYTES
                )));
            }
        }

        let body = String::from_utf8_lossy(&bytes).to_string();

        if raw {
            Ok(ToolResult::success(body))
        } else if content_type.contains("text/html") || body.contains("<!DOCTYPE") || body.contains("<html") {
            Ok(ToolResult::success(Self::strip_html(&body)))
        } else {
            // Return as-is for non-HTML content
            Ok(ToolResult::success(body))
        }
    }

    fn permission_level(&self) -> PermissionLevel {
        PermissionLevel::Ask
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_html_basic() {
        let html = "<p>Hello, <b>world</b>!</p>";
        let text = WebFetchTool::strip_html(html);
        assert!(text.contains("Hello"));
        assert!(text.contains("world"));
        assert!(!text.contains("<p>"));
        assert!(!text.contains("<b>"));
    }

    #[test]
    fn test_strip_html_script() {
        let html = "<p>Before</p><script>alert('xss')</script><p>After</p>";
        let text = WebFetchTool::strip_html(html);
        assert!(text.contains("Before"));
        assert!(text.contains("After"));
        assert!(!text.contains("alert"));
    }

    #[test]
    fn test_strip_html_style() {
        let html = "<style>.foo { color: red; }</style><p>Content</p>";
        let text = WebFetchTool::strip_html(html);
        assert!(text.contains("Content"));
        assert!(!text.contains("color"));
    }

    #[test]
    fn test_strip_html_entities() {
        let html = "<p>&amp; &lt; &gt; &quot; &nbsp;</p>";
        let text = WebFetchTool::strip_html(html);
        assert!(text.contains('&'));
        assert!(text.contains('<'));
        assert!(text.contains('>'));
        assert!(text.contains('"'));
    }

    #[test]
    fn test_strip_html_apos_entity() {
        // Bug fix R2: &apos; and &#39; should be decoded to apostrophe
        let html = "<p>it&apos;s working</p>";
        let text = WebFetchTool::strip_html(html);
        assert!(text.contains("it's working"), "got: {}", text);

        let html2 = "<p>it&#39;s also working</p>";
        let text2 = WebFetchTool::strip_html(html2);
        assert!(text2.contains("it's also working"), "got: {}", text2);
    }

    #[test]
    fn test_strip_html_empty() {
        assert_eq!(WebFetchTool::strip_html(""), "");
    }

    #[test]
    fn test_strip_html_plain_text() {
        let text = WebFetchTool::strip_html("Just plain text");
        assert_eq!(text, "Just plain text");
    }

    #[test]
    fn test_strip_html_whitespace_collapse() {
        let html = "<p>Hello    world    test</p>";
        let text = WebFetchTool::strip_html(html);
        // Whitespace should be collapsed
        assert!(!text.contains("    "));
    }

    #[test]
    fn test_tool_name() {
        let tool = WebFetchTool::new();
        assert_eq!(tool.name(), "web_fetch");
    }

    #[test]
    fn test_tool_schema() {
        let tool = WebFetchTool::new();
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["url"].is_object());
    }

    #[test]
    fn test_permission_level() {
        let tool = WebFetchTool::new();
        assert_eq!(tool.permission_level(), PermissionLevel::Ask);
    }

    #[tokio::test]
    async fn test_invalid_url() {
        let tool = WebFetchTool::new();
        let ctx = ToolContext::default();
        let result = tool
            .execute(serde_json::json!({"url": "ftp://example.com"}), &ctx)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_missing_url() {
        let tool = WebFetchTool::new();
        let ctx = ToolContext::default();
        let result = tool.execute(serde_json::json!({}), &ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_ssrf_localhost() {
        let tool = WebFetchTool::new();
        let ctx = ToolContext::default();
        let result = tool
            .execute(serde_json::json!({"url": "http://localhost:8080/admin"}), &ctx)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("SSRF"));
    }

    #[tokio::test]
    async fn test_ssrf_loopback() {
        let tool = WebFetchTool::new();
        let ctx = ToolContext::default();
        let result = tool
            .execute(serde_json::json!({"url": "http://127.0.0.1:9200/_cluster/health"}), &ctx)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("SSRF"));
    }

    #[tokio::test]
    async fn test_ssrf_metadata_endpoint() {
        let tool = WebFetchTool::new();
        let ctx = ToolContext::default();
        let result = tool
            .execute(serde_json::json!({"url": "http://169.254.169.254/latest/meta-data/"}), &ctx)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("SSRF"));
    }

    #[tokio::test]
    async fn test_ssrf_private_ip() {
        let tool = WebFetchTool::new();
        let ctx = ToolContext::default();
        let result = tool
            .execute(serde_json::json!({"url": "http://10.0.0.1/internal"}), &ctx)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("SSRF"));
    }

    #[tokio::test]
    async fn test_ssrf_ipv6_loopback() {
        let tool = WebFetchTool::new();
        let ctx = ToolContext::default();
        let result = tool
            .execute(serde_json::json!({"url": "http://[::1]:8080/admin"}), &ctx)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("SSRF"));
    }

    #[test]
    fn test_strip_html_utf8_truncation() {
        // Create a string with multi-byte UTF-8 characters that exceeds 50_000 bytes
        // Each CJK character is 3 bytes in UTF-8
        let html: String = std::iter::repeat('\u{4E2D}').take(20_000).collect(); // 60_000 bytes
        let result = WebFetchTool::strip_html(&html);
        // Should not panic and should be valid UTF-8
        assert!(result.contains("truncated"));
        assert!(result.is_char_boundary(0)); // valid UTF-8 overall
    }

    #[tokio::test]
    async fn test_ssrf_zero_ip() {
        let tool = WebFetchTool::new();
        let ctx = ToolContext::default();
        let result = tool
            .execute(serde_json::json!({"url": "http://0.0.0.0/"}), &ctx)
            .await;
        assert!(result.is_err());
    }
}
