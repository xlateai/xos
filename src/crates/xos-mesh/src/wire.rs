//! Wire JSON envelopes (v2: node ids, not rank indices).

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::nodes::{NodeEntry, NodeId, Roster};

pub const WIRE_VERSION: u32 = 2;

pub const ROSTER_KIND: &str = "__mesh_roster";

#[derive(Clone, Debug)]
pub struct Packet {
    pub from_node: NodeId,
    /// Zero-based index in sorted roster when known (UI hint only).
    pub from_display: Option<u32>,
    pub kind: String,
    pub body: serde_json::Value,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct WireEnvelope {
    pub v: u32,
    pub from: NodeId,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<NodeId>,
    pub payload: serde_json::Value,
}

pub fn should_deliver_locally(my_id: &str, from: &str, to: Option<&str>) -> bool {
    if from == my_id {
        return false;
    }
    match to {
        None => true,
        Some(t) => t == my_id,
    }
}

pub fn roster_payload(nodes: &[NodeEntry]) -> serde_json::Value {
    json!({
        "nodes": nodes.iter().map(|n| json!({"id": n.id, "started_at": n.started_at})).collect::<Vec<_>>(),
    })
}

pub fn parse_roster_payload(v: &serde_json::Value) -> Option<Roster> {
    let arr = v.get("nodes")?.as_array()?;
    let mut out = Vec::new();
    for x in arr {
        let id = x.get("id")?.as_str()?.to_string();
        let started_at = x.get("started_at")?.as_u64()?;
        out.push(NodeEntry { id, started_at });
    }
    Some(Roster::from_nodes(out))
}
