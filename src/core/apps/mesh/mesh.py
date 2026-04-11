"""
Mesh chat demo — run `xos app mesh` (or `xos rs mesh`).

- ``mode="local"``: loopback only (same machine).
- ``mode="lan"``: coordinator listens on all interfaces; peers find it via UDP broadcast on a
  derived port (same ``id``). No manual IP. Edit ``CHAT_ID`` / ``MODE`` below.
"""

import xos

CHAT_ID = "chat-demo"
MODE = "local"


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
