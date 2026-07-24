//! MemoryPort — ingestion boundary for thread-to-memory wiring (D6).
//!
//! When a zed-kask agent thread completes a turn, the conversation is offered
//! to the memory system for episodic + semantic ingestion. This port is the
//! hexagonal boundary: the `agent` crate calls it (via a global hook, same
//! pattern as `set_manifest_executor`), and the bridge provides the
//! implementation.
//!
//! The `TurnRecord` schema matches hKask's `ChatTurn` type
//! (`{"user_input": ..., "agent_response": ...}`) so when the full memory
//! stack is wired, ingestion will be compatible with `ChatTurn::from_value()`.
//!
//! The initial bridge implementation is a logging no-op — the full hKask
//! memory stack (SQLCipher, episodic/semantic storage, consolidation) is
//! deferred until the storage layer and WebID mapping are available in-process.

use std::future::Future;
use std::pin::Pin;

/// A completed turn offered to the memory system for ingestion.
///
/// Field names align with hKask's `ChatTurn` schema (`user_input`,
/// `agent_response`) so the `serde_json::json!` serialization is compatible
/// with `ChatTurn::from_value()`.
#[derive(Debug, Clone)]
pub struct TurnRecord {
    /// The thread/session identifier (zed's `SessionId` as a string).
    /// Maps to the h_mem `entity` field.
    pub thread_id: String,
    /// The user's input text for this turn.
    /// Maps to `ChatTurn.user_input`.
    pub user_input: String,
    /// The agent's response text for this turn.
    /// Maps to `ChatTurn.agent_response`.
    pub agent_response: String,
    /// The model that produced the response (e.g., "claude-sonnet-4-20250514").
    pub model: String,
    /// Optional thread title (if available).
    pub thread_title: Option<String>,
}

impl TurnRecord {
    /// Serialize to the hKask `ChatTurn` JSON schema: `{"user_input": ..., "agent_response": ...}`.
    ///
    /// This is the value stored in the h_mem `value` field. The `thread_id`
    /// becomes the h_mem `entity`, and `"chatted"` is the h_mem `attribute`.
    pub fn to_chat_turn_value(&self) -> serde_json::Value {
        serde_json::json!({
            "user_input": self.user_input,
            "agent_response": self.agent_response,
        })
    }
}

/// Error type for memory ingestion failures.
#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error("memory ingestion failed: {0}")]
    Ingestion(String),
}

/// Pinned boxed future for dyn-compatibility.
pub type MemoryFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Port for ingesting completed thread turns into hKask memory (D6).
///
/// The bridge provides the implementation. When no implementation is injected
/// (standalone or first-run), ingestion is a no-op.
///
/// The ingestion pattern mirrors hKask's `DaemonHandler::store_experience`:
/// - Episodic: stored as a private, perspective-scoped h_mem with entity=thread_id,
///   attribute="chatted", value=`TurnRecord::to_chat_turn_value()`
/// - Semantic: stored as a shared h_mem (requires consolidation capability)
/// - Confidence: derived from experience classification (deferred)
pub trait MemoryPort: Send + Sync {
    /// Ingest a completed turn into episodic (and optionally semantic) memory.
    ///
    /// This is fire-and-forget from the caller's perspective — the memory system
    /// handles classification, confidence scoring, and consolidation asynchronously.
    fn ingest_turn<'a>(&'a self, record: TurnRecord) -> MemoryFuture<'a, Result<(), MemoryError>>;
}
