//! Logical full mesh overlay: every node is reachable via the coordinator (star on the wire).

use super::nodes::{NodeEntry, NodeId, Roster};

/// Complete-graph view over the current roster (every id is a “neighbor” of every other).
#[derive(Clone, Debug)]
pub struct MeshGraph {
    roster: Roster,
}

impl MeshGraph {
    pub fn new(roster: Roster) -> Self {
        Self { roster }
    }

    pub fn roster(&self) -> &Roster {
        &self.roster
    }

    /// All other node ids (excluding `exclude` if present).
    pub fn neighbor_ids(&self, exclude: Option<&NodeId>) -> Vec<&str> {
        self.roster
            .entries()
            .iter()
            .map(|e: &NodeEntry| e.id.as_str())
            .filter(|id| exclude.map(|x| x != *id).unwrap_or(true))
            .collect()
    }
}
