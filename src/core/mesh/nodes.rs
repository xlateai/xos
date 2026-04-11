//! Stable node identity (`node_id`) and roster ordering by `started_at` (then id).

use serde::{Deserialize, Serialize};

/// Globally unique node id (RFC 4122-style UUID v4 string).
pub type NodeId = String;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeEntry {
    pub id: NodeId,
    /// Milliseconds since Unix epoch — used only for **display order**, not as a wire index.
    pub started_at: u64,
}

/// Authoritative view of participants (coordinator + peers). Order is deterministic.
#[derive(Clone, Debug, Default)]
pub struct Roster {
    entries: Vec<NodeEntry>,
}

impl Roster {
    pub fn from_nodes(mut nodes: Vec<NodeEntry>) -> Self {
        nodes.sort_by(|a, b| {
            a.started_at
                .cmp(&b.started_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        Self { entries: nodes }
    }

    pub fn entries(&self) -> &[NodeEntry] {
        &self.entries
    }

    /// Zero-based position in the sorted roster (optional “rank” for UI).
    pub fn display_index(&self, id: &str) -> Option<u32> {
        self.entries
            .iter()
            .position(|e| e.id == id)
            .map(|i| i as u32)
    }

    pub fn len(&self) -> u32 {
        self.entries.len() as u32
    }
}

/// New random node id (UUID v4).
pub fn new_node_id() -> NodeId {
    uuid::Uuid::new_v4().to_string()
}

pub fn now_unix_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
