//! Loopback TCP port selection from a mesh id (deterministic hash).

/// TCP port for a logical mesh room (`mesh_id` string).
pub(super) fn port_for_mesh_id(mesh_id: &str) -> u16 {
    let mut h: u32 = 2166136261;
    for b in mesh_id.bytes() {
        h = h.wrapping_mul(16777619);
        h ^= b as u32;
    }
    40_000u16.saturating_add((h % 25_000) as u16)
}
