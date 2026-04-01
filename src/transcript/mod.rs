//! Transcript store for conversation history management
//!
//! Provides append, compact, replay, and flush operations
//! with dirty tracking for persistence.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A single entry in the transcript
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptEntry {
    /// Timestamp of the entry
    pub timestamp: DateTime<Utc>,
    /// Role: "user", "assistant", or "tool"
    pub role: String,
    /// Content text
    pub content: String,
    /// Optional tool name (for tool results)
    pub tool_name: Option<String>,
    /// Estimated token count for this entry
    pub token_estimate: usize,
}

impl TranscriptEntry {
    /// Create a new transcript entry
    pub fn new(role: impl Into<String>, content: impl Into<String>) -> Self {
        let content = content.into();
        let token_estimate = estimate_tokens(&content);
        Self {
            timestamp: Utc::now(),
            role: role.into(),
            content,
            tool_name: None,
            token_estimate,
        }
    }

    /// Create a tool result entry
    pub fn tool(tool_name: impl Into<String>, content: impl Into<String>) -> Self {
        let content = content.into();
        let token_estimate = estimate_tokens(&content);
        let tool_name = tool_name.into();
        Self {
            timestamp: Utc::now(),
            role: "tool".to_string(),
            content,
            tool_name: Some(tool_name),
            token_estimate,
        }
    }
}

/// Estimate token count (~4 chars per token)
fn estimate_tokens(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    let char_count = text.chars().count();
    (char_count + 3) / 4
}

/// In-memory transcript store with dirty tracking
pub struct TranscriptStore {
    /// All entries in the transcript
    entries: Vec<TranscriptEntry>,
    /// Whether the store has unsaved changes
    dirty: bool,
    /// Index of last flushed entry (entries before this are persisted)
    flush_index: usize,
}

impl TranscriptStore {
    /// Create a new empty transcript store
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            dirty: false,
            flush_index: 0,
        }
    }

    /// Append an entry to the transcript
    pub fn append(&mut self, entry: TranscriptEntry) {
        self.entries.push(entry);
        self.dirty = true;
    }

    /// Append a user message
    pub fn append_user(&mut self, content: impl Into<String>) {
        self.append(TranscriptEntry::new("user", content));
    }

    /// Append an assistant message
    pub fn append_assistant(&mut self, content: impl Into<String>) {
        self.append(TranscriptEntry::new("assistant", content));
    }

    /// Append a tool result
    pub fn append_tool(&mut self, tool_name: impl Into<String>, content: impl Into<String>) {
        self.append(TranscriptEntry::tool(tool_name, content));
    }

    /// Compact the transcript to keep only the last N entries
    /// Returns the number of entries removed
    pub fn compact(&mut self, keep_last: usize) -> usize {
        if self.entries.len() <= keep_last {
            return 0;
        }
        let removed = self.entries.len() - keep_last;
        self.entries.drain(..removed);
        // Adjust flush_index
        self.flush_index = self.flush_index.saturating_sub(removed);
        self.dirty = true;
        removed
    }

    /// Replay all entries as formatted strings
    pub fn replay(&self) -> Vec<String> {
        self.entries
            .iter()
            .map(|e| {
                let prefix = match e.role.as_str() {
                    "user" => "User",
                    "assistant" => "Assistant",
                    "tool" => {
                        if let Some(ref name) = e.tool_name {
                            return format!("[Tool: {}] {}", name, e.content);
                        }
                        "Tool"
                    }
                    other => other,
                };
                format!("{}: {}", prefix, e.content)
            })
            .collect()
    }

    /// Mark all current entries as persisted
    pub fn flush(&mut self) {
        self.flush_index = self.entries.len();
        self.dirty = false;
    }

    /// Get entries that haven't been flushed yet
    pub fn unflushed(&self) -> &[TranscriptEntry] {
        &self.entries[self.flush_index..]
    }

    /// Check if there are unsaved changes
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Get the total number of entries
    pub fn size(&self) -> usize {
        self.entries.len()
    }

    /// Estimate total token count across all entries
    pub fn total_token_estimate(&self) -> usize {
        self.entries.iter().map(|e| e.token_estimate).sum()
    }

    /// Get all entries
    pub fn entries(&self) -> &[TranscriptEntry] {
        &self.entries
    }

    /// Clear all entries
    pub fn clear(&mut self) {
        self.entries.clear();
        self.flush_index = 0;
        self.dirty = false;
    }
}

impl Default for TranscriptStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_transcript_empty() {
        let store = TranscriptStore::new();
        assert_eq!(store.size(), 0);
        assert!(!store.is_dirty());
        assert_eq!(store.total_token_estimate(), 0);
    }

    #[test]
    fn test_append_marks_dirty() {
        let mut store = TranscriptStore::new();
        store.append_user("hello");
        assert!(store.is_dirty());
        assert_eq!(store.size(), 1);
    }

    #[test]
    fn test_append_user_and_assistant() {
        let mut store = TranscriptStore::new();
        store.append_user("hello");
        store.append_assistant("hi there");
        assert_eq!(store.size(), 2);
        assert_eq!(store.entries()[0].role, "user");
        assert_eq!(store.entries()[1].role, "assistant");
    }

    #[test]
    fn test_append_tool() {
        let mut store = TranscriptStore::new();
        store.append_tool("bash", "file1.rs\nfile2.rs");
        assert_eq!(store.size(), 1);
        assert_eq!(store.entries()[0].role, "tool");
        assert_eq!(store.entries()[0].tool_name, Some("bash".to_string()));
    }

    #[test]
    fn test_compact() {
        let mut store = TranscriptStore::new();
        for i in 0..10 {
            store.append_user(format!("message {}", i));
        }
        let removed = store.compact(3);
        assert_eq!(removed, 7);
        assert_eq!(store.size(), 3);
        assert_eq!(store.entries()[0].content, "message 7");
    }

    #[test]
    fn test_compact_no_op() {
        let mut store = TranscriptStore::new();
        store.append_user("hello");
        let removed = store.compact(10);
        assert_eq!(removed, 0);
        assert_eq!(store.size(), 1);
    }

    #[test]
    fn test_replay() {
        let mut store = TranscriptStore::new();
        store.append_user("hello");
        store.append_assistant("hi");
        store.append_tool("bash", "output");
        let replay = store.replay();
        assert_eq!(replay.len(), 3);
        assert_eq!(replay[0], "User: hello");
        assert_eq!(replay[1], "Assistant: hi");
        assert_eq!(replay[2], "[Tool: bash] output");
    }

    #[test]
    fn test_flush_and_unflushed() {
        let mut store = TranscriptStore::new();
        store.append_user("first");
        store.flush();
        assert!(!store.is_dirty());
        assert_eq!(store.unflushed().len(), 0);

        store.append_user("second");
        assert!(store.is_dirty());
        assert_eq!(store.unflushed().len(), 1);
        assert_eq!(store.unflushed()[0].content, "second");
    }

    #[test]
    fn test_total_token_estimate() {
        let mut store = TranscriptStore::new();
        store.append_user("hello world test message");
        assert!(store.total_token_estimate() > 0);
    }

    #[test]
    fn test_clear() {
        let mut store = TranscriptStore::new();
        store.append_user("hello");
        store.append_assistant("hi");
        store.clear();
        assert_eq!(store.size(), 0);
        assert!(!store.is_dirty());
    }

    #[test]
    fn test_compact_adjusts_flush_index() {
        let mut store = TranscriptStore::new();
        for i in 0..5 {
            store.append_user(format!("msg {}", i));
        }
        store.flush();
        assert_eq!(store.unflushed().len(), 0);

        // Compact to keep 2
        store.compact(2);
        // flush_index should be adjusted
        assert_eq!(store.size(), 2);
    }

    #[test]
    fn test_entry_timestamp() {
        let entry = TranscriptEntry::new("user", "hello");
        // Timestamp should be recent
        let diff = Utc::now() - entry.timestamp;
        assert!(diff.num_seconds() < 2);
    }

    #[test]
    fn test_estimate_tokens() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("hello world"), 3); // 11 chars -> (11+3)/4 = 3
        assert!(estimate_tokens("a longer message with more words") > 0);
    }
}
