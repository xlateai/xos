"""
Local mesh chat — run `xos app mesh` (or `xos rs mesh`) in two or more terminals on the same machine.
"""

import xos

SESSION = "chat-demo"


def main() -> None:
    mesh = xos.mesh.connect(SESSION)
    rank = mesh.rank()
    nodes = mesh.num_nodes()
    print(f"[mesh] rank={rank}  nodes={nodes}  session={SESSION!r}")
    print("Type a line and press Enter to broadcast. Ctrl+C to exit.\n")

    while True:
        line = xos.input(">>> ", wait=False)
        if line is not None:
            mesh.broadcast(id="message", msg=line)

        packets = mesh.receive(id="message", wait=False, latest_only=False)
        if packets:
            for packet in packets:
                who = getattr(packet, "from_rank", "?")
                text = getattr(packet, "msg", "")
                print(f"[{who}] {text}")


main()
