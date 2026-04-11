"""
Mesh chat demo — run `xos app mesh` (or `xos rs mesh`).

- ``mode="local"``: loopback only (same machine).
- ``mode="lan"``: tries **loopback first** (two terminals on one PC), then UDP discovery on Wi‑Fi,
  then binds as coordinator if alone. Same ``CHAT_ID`` must match. Edit ``CHAT_ID`` / ``MODE`` below.
"""

import xos

CHAT_ID = "chat-demo"
MODE = "lan"


def main() -> None:
    mesh = xos.mesh.connect(id=CHAT_ID, mode=MODE)
    rank = mesh.rank()
    nodes = mesh.num_nodes()
    print(f"[mesh] rank={rank}  nodes={nodes}  id={CHAT_ID!r}  mode={MODE!r}")
    print("Type a line and press Enter to broadcast. Ctrl+C to exit.\n")

    while True:
        # Drain inbound first so chat updates without waiting for local input.
        packets = mesh.receive(id="message", wait=False, latest_only=False)
        if packets:
            for packet in packets:
                who = getattr(packet, "from_rank", "?")
                text = getattr(packet, "msg", "")
                print(f"[{who}] {text}")

        line = xos.input(">>> ", wait=False)
        if line is not None:
            mesh.broadcast(id="message", msg=line)


main()
