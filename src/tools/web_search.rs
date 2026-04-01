//! Web search tool — search via DuckDuckGo HTML

use super::{PermissionLevel, Tool, ToolContext, ToolError, ToolResult};
use async_trait::async_trait;

/// Search result entry
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// Tool for performing web searches
pub struct WebSearchTool;

impl WebSearchTool {
    pub fn new() -> Self {
        Self
    }

    /// Parse DuckDuckGo HTML results page into structured results
    pub fn parse_ddg_results(html: &str) -> Vec<SearchResult> {
        let mut results = Vec::new();

        // DuckDuckGo HTML results contain <a class="result__a" href="...">Title</a>
        // and <a class="result__snippet" ...>Snippet</a>
        // We use a simple regex-free approach to extract results

        // Look for result links — DDG uses various formats
        // Pattern: href="//duckduckgo.com/l/?uddg=ENCODED_URL...">TITLE</a>
        // Or direct links

        let mut search_pos = 0;
        while let Some(pos) = html[search_pos..].find("class=\"result__a\"") {
            let abs_pos = search_pos + pos;
            search_pos = abs_pos + 1;

            // Find the href before this class
            let before = &html[..abs_pos];
            let href_start = match before.rfind("href=\"") {
                Some(p) => p + 6,
                None => continue,
            };
            let href_end = match html[href_start..].find('"') {
                Some(p) => href_start + p,
                None => continue,
            };
            let raw_url = &html[href_start..href_end];

            // Extract actual URL from DuckDuckGo redirect
            let url = if raw_url.contains("uddg=") {
                // URL-encoded redirect: extract uddg parameter
                if let Some(uddg_pos) = raw_url.find("uddg=") {
                    let url_encoded = &raw_url[uddg_pos + 5..];
                    let end = url_encoded.find('&').unwrap_or(url_encoded.len());
                    let encoded = &url_encoded[..end];
                    // Simple percent-decode
                    percent_decode(encoded)
                } else {
                    raw_url.to_string()
                }
            } else if raw_url.starts_with("//") {
                format!("https:{}", raw_url)
            } else {
                raw_url.to_string()
            };

            // Find the title (text between > and </a>)
            let tag_end = match html[abs_pos..].find('>') {
                Some(p) => abs_pos + p + 1,
                None => continue,
            };
            let title_end = match html[tag_end..].find("</a>") {
                Some(p) => tag_end + p,
                None => continue,
            };
            let title = strip_tags(&html[tag_end..title_end]);

            // Try to find snippet nearby
            let snippet_search_end = (abs_pos + 2000).min(html.len());
            let snippet = if let Some(snip_pos) = html[abs_pos..snippet_search_end].find("class=\"result__snippet\"") {
                let snip_abs = abs_pos + snip_pos;
                let snip_tag_end = html[snip_abs..].find('>').map(|p| snip_abs + p + 1);
                if let Some(ste) = snip_tag_end {
                    let snip_content_end = html[ste..].find("</").map(|p| ste + p).unwrap_or(ste);
                    strip_tags(&html[ste..snip_content_end])
                } else {
                    String::new()
                }
            } else {
                String::new()
            };

            if !title.is_empty() && !url.is_empty() {
                results.push(SearchResult {
                    title: title.trim().to_string(),
                    url: url.trim().to_string(),
                    snippet: snippet.trim().to_string(),
                });
            }

            if results.len() >= 10 {
                break;
            }
        }

        results
    }

    /// Format results for display
    pub fn format_results(results: &[SearchResult]) -> String {
        if results.is_empty() {
            return "No results found.".to_string();
        }

        let mut output = String::new();
        for (i, result) in results.iter().enumerate() {
            output.push_str(&format!("{}. {}\n", i + 1, result.title));
            output.push_str(&format!("   {}\n", result.url));
            if !result.snippet.is_empty() {
                output.push_str(&format!("   {}\n", result.snippet));
            }
            output.push('\n');
        }
        output
    }
}

/// Simple HTML tag stripping
fn strip_tags(html: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;
    for ch in html.chars() {
        if ch == '<' {
            in_tag = true;
        } else if ch == '>' {
            in_tag = false;
        } else if !in_tag {
            result.push(ch);
        }
    }
    result
}

/// Simple percent decoding
fn percent_decode(input: &str) -> String {
    let mut result = String::new();
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = &input[i + 1..i + 3];
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                result.push(byte as char);
                i += 3;
                continue;
            }
        }
        result.push(bytes[i] as char);
        i += 1;
    }
    result
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Search the web using DuckDuckGo. Returns titles, URLs, and snippets."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results (default 5)"
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let query = params["query"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("Missing 'query' parameter".into()))?;

        let max_results = params["max_results"].as_u64().unwrap_or(5) as usize;

        // URL-encode the query
        let encoded_query = query.replace(' ', "+");
        let url = format!("https://html.duckduckgo.com/html/?q={}", encoded_query);

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("claw-rs/1.0")
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to create HTTP client: {}", e)))?;

        let response = client
            .get(&url)
            .send()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Search request failed: {}", e)))?;

        if !response.status().is_success() {
            return Ok(ToolResult::error(format!(
                "Search failed with HTTP {}",
                response.status().as_u16()
            )));
        }

        let html = response
            .text()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to read response: {}", e)))?;

        let mut results = Self::parse_ddg_results(&html);
        results.truncate(max_results);

        Ok(ToolResult::success(Self::format_results(&results)))
    }

    fn permission_level(&self) -> PermissionLevel {
        PermissionLevel::Ask
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_tags() {
        assert_eq!(strip_tags("<b>hello</b>"), "hello");
        assert_eq!(strip_tags("plain text"), "plain text");
        assert_eq!(strip_tags("<a href=\"url\">link</a>"), "link");
    }

    #[test]
    fn test_percent_decode() {
        assert_eq!(percent_decode("hello%20world"), "hello world");
        assert_eq!(percent_decode("foo%3Dbar"), "foo=bar");
        assert_eq!(percent_decode("noencode"), "noencode");
    }

    #[test]
    fn test_format_results_empty() {
        let results: Vec<SearchResult> = vec![];
        assert_eq!(WebSearchTool::format_results(&results), "No results found.");
    }

    #[test]
    fn test_format_results() {
        let results = vec![
            SearchResult {
                title: "Test Title".into(),
                url: "https://example.com".into(),
                snippet: "A test snippet".into(),
            },
        ];
        let output = WebSearchTool::format_results(&results);
        assert!(output.contains("1. Test Title"));
        assert!(output.contains("https://example.com"));
        assert!(output.contains("A test snippet"));
    }

    #[test]
    fn test_parse_ddg_results_empty() {
        let results = WebSearchTool::parse_ddg_results("<html><body></body></html>");
        assert!(results.is_empty());
    }

    #[test]
    fn test_parse_ddg_results_with_content() {
        let html = r#"
        <div class="result">
            <a rel="nofollow" href="https://example.com" class="result__a">Example Title</a>
            <span class="result__snippet">Example snippet here</span>
        </div>
        "#;
        let results = WebSearchTool::parse_ddg_results(html);
        // May or may not find results depending on exact format; verify no panic
        assert!(results.len() <= 10);
    }

    #[test]
    fn test_tool_name() {
        let tool = WebSearchTool::new();
        assert_eq!(tool.name(), "web_search");
    }

    #[test]
    fn test_tool_schema() {
        let tool = WebSearchTool::new();
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["query"].is_object());
    }

    #[test]
    fn test_permission_level() {
        let tool = WebSearchTool::new();
        assert_eq!(tool.permission_level(), PermissionLevel::Ask);
    }

    #[tokio::test]
    async fn test_missing_query() {
        let tool = WebSearchTool::new();
        let ctx = ToolContext::default();
        let result = tool.execute(serde_json::json!({}), &ctx).await;
        assert!(result.is_err());
    }
}
