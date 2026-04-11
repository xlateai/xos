"""
Mesh chat demo — run `xos app mesh` (or `xos rs mesh`).

- ``mode="local"``: loopback only (same machine).
- ``mode="lan"``: tries **loopback first** (two terminals on one PC), then UDP discovery on Wi‑Fi,
  then binds as coordinator if alone. Same ``CHAT_ID`` must match. Edit ``CHAT_ID`` / ``MODE`` below.
"""

import xos

CHAT_ID = "chat-demo"
# "local" = plaintext, same-machine. "lan" = encrypted + discovery (also tries loopback first).
MODE = "lan"


def main() -> None:
    mesh = xos.mesh.connect(id=CHAT_ID, mode=MODE)
    print(
        f"[mesh] rank={mesh.rank()}  nodes={mesh.num_nodes()}  "
        f"id={CHAT_ID!r}  mode={MODE!r}"
    )
    print("Type a line and press Enter to broadcast. Ctrl+C to exit.\n")

    while True:
        # Drain inbound first so chat updates without waiting for local input.
        packets = mesh.receive(id="message", wait=False, latest_only=False)
        if packets:
            for packet in packets:
                sid = getattr(packet, "from_id", "") or ""
                short = (sid[:8] + "…") if len(sid) >= 8 else (sid or "?")
                text = getattr(packet, "msg", "")
                print(f"[rank {getattr(packet, 'from_rank', '?')} id {short}] {text}")

        line = xos.input(mesh.prompt(), wait=False)
        if line is not None:
            mesh.broadcast(id="message", msg=line)


main()
