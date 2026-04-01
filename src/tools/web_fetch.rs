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

        // Limit output length
        if trimmed.len() > 50_000 {
            format!("{}...\n\n(truncated at 50000 characters)", &trimmed[..50_000])
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

        // Read body with size limit
        let bytes = response
            .bytes()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to read response body: {}", e)))?;

        if bytes.len() > Self::MAX_BODY_BYTES {
            return Ok(ToolResult::error(format!(
                "Response too large: {} bytes (max {})",
                bytes.len(),
                Self::MAX_BODY_BYTES
            )));
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
}
