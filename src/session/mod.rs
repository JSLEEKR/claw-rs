//! Session management — save/load conversation history

use crate::config::SessionConfig;
use crate::llm::{Message, Usage};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Session error
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("Session not found: {0}")]
    NotFound(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// A conversation session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique session ID
    pub id: String,

    /// When the session was created
    pub created_at: DateTime<Utc>,

    /// When the session was last updated
    pub updated_at: DateTime<Utc>,

    /// Conversation messages
    pub messages: Vec<Message>,

    /// Cumulative token usage
    pub usage: Usage,

    /// Session metadata
    pub metadata: SessionMetadata,
}

/// Session metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    /// Model used
    pub model: String,

    /// Working directory
    pub cwd: String,

    /// Number of turns completed
    pub turn_count: usize,
}

impl Session {
    /// Create a new session
    pub fn new(model: &str, cwd: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            messages: Vec::new(),
            usage: Usage::default(),
            metadata: SessionMetadata {
                model: model.to_string(),
                cwd: cwd.to_string(),
                turn_count: 0,
            },
        }
    }

    /// Create a session with a specific ID (for testing or resume)
    pub fn with_id(id: &str, model: &str, cwd: &str) -> Self {
        Self {
            id: id.to_string(),
            ..Self::new(model, cwd)
        }
    }

    /// Add a message to the session
    pub fn add_message(&mut self, message: Message) {
        self.messages.push(message);
        self.updated_at = Utc::now();
    }

    /// Update usage statistics
    pub fn update_usage(&mut self, usage: &Usage) {
        self.usage.add(usage);
        self.metadata.turn_count += 1;
        self.updated_at = Utc::now();
    }

    /// Get the number of messages
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    /// Compact messages by keeping only the last N
    pub fn compact(&mut self, keep_last: usize) {
        if self.messages.len() > keep_last {
            let drain_count = self.messages.len() - keep_last;
            self.messages.drain(..drain_count);
            self.updated_at = Utc::now();
        }
    }

    /// Clear all messages
    pub fn clear(&mut self) {
        self.messages.clear();
        self.updated_at = Utc::now();
    }
}

/// Session store for persisting sessions to disk
pub struct SessionStore {
    storage_dir: PathBuf,
}

impl SessionStore {
    /// Create a new session store
    pub fn new(config: &SessionConfig) -> Result<Self, SessionError> {
        let store = Self {
            storage_dir: config.storage_dir.clone(),
        };
        // Create storage directory if it doesn't exist
        if !store.storage_dir.exists() {
            std::fs::create_dir_all(&store.storage_dir)?;
        }
        Ok(store)
    }

    /// Create a session store with a specific directory (for testing)
    pub fn with_dir(dir: PathBuf) -> Result<Self, SessionError> {
        if !dir.exists() {
            std::fs::create_dir_all(&dir)?;
        }
        Ok(Self { storage_dir: dir })
    }

    /// Save a session to disk
    pub async fn save(&self, session: &Session) -> Result<(), SessionError> {
        let path = self.session_path(&session.id);
        let json = serde_json::to_string_pretty(session)
            .map_err(|e| SessionError::Serialization(e.to_string()))?;
        tokio::fs::write(&path, json).await?;
        Ok(())
    }

    /// Load a session from disk
    pub async fn load(&self, session_id: &str) -> Result<Session, SessionError> {
        let path = self.session_path(session_id);
        if !path.exists() {
            return Err(SessionError::NotFound(session_id.to_string()));
        }
        let json = tokio::fs::read_to_string(&path).await?;
        serde_json::from_str(&json).map_err(|e| SessionError::Serialization(e.to_string()))
    }

    /// List all session IDs
    pub fn list_sessions(&self) -> Result<Vec<String>, SessionError> {
        let mut sessions = Vec::new();
        for entry in std::fs::read_dir(&self.storage_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "json") {
                if let Some(name) = path.file_stem() {
                    sessions.push(name.to_string_lossy().to_string());
                }
            }
        }
        sessions.sort();
        Ok(sessions)
    }

    /// Delete a session
    pub async fn delete(&self, session_id: &str) -> Result<(), SessionError> {
        let path = self.session_path(session_id);
        if path.exists() {
            tokio::fs::remove_file(&path).await?;
        }
        Ok(())
    }

    /// Check if a session exists
    pub fn exists(&self, session_id: &str) -> bool {
        self.session_path(session_id).exists()
    }

    /// Get the file path for a session
    ///
    /// Sanitizes session_id to prevent path traversal attacks (e.g., "../../etc/crontab").
    fn session_path(&self, session_id: &str) -> PathBuf {
        // Strip any path separators and ".." to prevent directory traversal
        let sanitized: String = session_id
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
            .take(255) // Limit length to prevent excessively long filenames
            .collect();
        let safe_id = if sanitized.is_empty() { "invalid" } else { &sanitized };
        self.storage_dir.join(format!("{}.json", safe_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_creation() {
        let session = Session::new("claude-3", "/home/user");
        assert!(!session.id.is_empty());
        assert_eq!(session.metadata.model, "claude-3");
        assert_eq!(session.metadata.cwd, "/home/user");
        assert_eq!(session.message_count(), 0);
        assert_eq!(session.metadata.turn_count, 0);
    }

    #[test]
    fn test_session_with_id() {
        let session = Session::with_id("test-id", "model", "/");
        assert_eq!(session.id, "test-id");
    }

    #[test]
    fn test_add_message() {
        let mut session = Session::new("model", "/");
        session.add_message(Message::user("hello"));
        assert_eq!(session.message_count(), 1);
        session.add_message(Message::assistant("hi"));
        assert_eq!(session.message_count(), 2);
    }

    #[test]
    fn test_update_usage() {
        let mut session = Session::new("model", "/");
        let usage = Usage {
            input_tokens: 100,
            output_tokens: 50,
        };
        session.update_usage(&usage);
        assert_eq!(session.usage.input_tokens, 100);
        assert_eq!(session.usage.output_tokens, 50);
        assert_eq!(session.metadata.turn_count, 1);

        session.update_usage(&usage);
        assert_eq!(session.usage.input_tokens, 200);
        assert_eq!(session.metadata.turn_count, 2);
    }

    #[test]
    fn test_compact() {
        let mut session = Session::new("model", "/");
        for i in 0..10 {
            session.add_message(Message::user(format!("msg {}", i)));
        }
        assert_eq!(session.message_count(), 10);

        session.compact(5);
        assert_eq!(session.message_count(), 5);
        // Should keep last 5
        assert_eq!(session.messages[0].text_content(), "msg 5");
    }

    #[test]
    fn test_compact_no_op() {
        let mut session = Session::new("model", "/");
        session.add_message(Message::user("hello"));
        session.compact(10);
        assert_eq!(session.message_count(), 1);
    }

    #[test]
    fn test_clear() {
        let mut session = Session::new("model", "/");
        session.add_message(Message::user("hello"));
        session.clear();
        assert_eq!(session.message_count(), 0);
    }

    #[tokio::test]
    async fn test_session_store_save_load() {
        let dir = tempfile::tempdir().unwrap();
        let store = SessionStore::with_dir(dir.path().to_path_buf()).unwrap();

        let mut session = Session::with_id("test-session", "model", "/");
        session.add_message(Message::user("hello"));

        store.save(&session).await.unwrap();
        assert!(store.exists("test-session"));

        let loaded = store.load("test-session").await.unwrap();
        assert_eq!(loaded.id, "test-session");
        assert_eq!(loaded.message_count(), 1);
    }

    #[tokio::test]
    async fn test_session_store_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let store = SessionStore::with_dir(dir.path().to_path_buf()).unwrap();

        let result = store.load("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_session_store_list() {
        let dir = tempfile::tempdir().unwrap();
        let store = SessionStore::with_dir(dir.path().to_path_buf()).unwrap();

        let s1 = Session::with_id("session-a", "model", "/");
        let s2 = Session::with_id("session-b", "model", "/");
        store.save(&s1).await.unwrap();
        store.save(&s2).await.unwrap();

        let list = store.list_sessions().unwrap();
        assert_eq!(list.len(), 2);
        assert!(list.contains(&"session-a".to_string()));
        assert!(list.contains(&"session-b".to_string()));
    }

    #[tokio::test]
    async fn test_session_store_delete() {
        let dir = tempfile::tempdir().unwrap();
        let store = SessionStore::with_dir(dir.path().to_path_buf()).unwrap();

        let session = Session::with_id("to-delete", "model", "/");
        store.save(&session).await.unwrap();
        assert!(store.exists("to-delete"));

        store.delete("to-delete").await.unwrap();
        assert!(!store.exists("to-delete"));
    }

    #[tokio::test]
    async fn test_session_store_delete_nonexistent() {
        let dir = tempfile::tempdir().unwrap();
        let store = SessionStore::with_dir(dir.path().to_path_buf()).unwrap();
        // Should not error
        store.delete("nonexistent").await.unwrap();
    }

    #[test]
    fn test_session_path_traversal_sanitized() {
        let dir = tempfile::tempdir().unwrap();
        let store = SessionStore::with_dir(dir.path().to_path_buf()).unwrap();
        // Malicious session ID with path traversal should be sanitized
        let path = store.session_path("../../etc/crontab");
        let path_str = path.to_string_lossy();
        // Path separators and dots are stripped, no traversal possible
        assert!(!path_str.contains(".."));
        assert!(!path_str.contains('/') || path_str.starts_with(&dir.path().to_string_lossy().to_string()));
        // The sanitized name should not contain path separators
        let file_name = path.file_stem().unwrap().to_string_lossy();
        assert!(!file_name.contains('/'));
        assert!(!file_name.contains('\\'));
        assert!(!file_name.contains(".."));
        // Result should be within storage dir
        assert!(path.starts_with(dir.path()));
    }

    #[test]
    fn test_session_id_length_limited() {
        let dir = tempfile::tempdir().unwrap();
        let store = SessionStore::with_dir(dir.path().to_path_buf()).unwrap();
        let long_id = "a".repeat(1000);
        let path = store.session_path(&long_id);
        let file_name = path.file_stem().unwrap().to_string_lossy();
        // Should be truncated to 255 characters max
        assert!(file_name.len() <= 255);
    }

    #[test]
    fn test_session_serialization() {
        let mut session = Session::new("model", "/");
        session.add_message(Message::user("hello"));
        let json = serde_json::to_string(&session).unwrap();
        let parsed: Session = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, session.id);
        assert_eq!(parsed.message_count(), 1);
    }
}
