"""
`xos terminal` prototype.

Simple full-screen mesh console:
- joins a shared global channel on startup
- renders a live status bar at the top
- provides a lightweight mesh chat area at the bottom
"""

import time

import xos

MESH_ID = "xos-global"
MODE = "lan"

STATUS_REFRESH_SECS = 0.2
LOOP_SLEEP_SECS = 0.05
MAX_LOG_LINES = 200
DEFAULT_WIDTH = 120
DEFAULT_HEIGHT = 30


def _short_node_id(node_id: str) -> str:
    if not node_id:
        return "?"
    if len(node_id) <= 12:
        return node_id
    return node_id[:12] + "..."


def _fit(text: str, width: int) -> str:
    if width <= 0:
        return ""
    if len(text) <= width:
        return text + (" " * (width - len(text)))
    if width <= 3:
        return "."
    return text[: width - 3] + "..."


def _stamp() -> str:
    return time.strftime("%H:%M:%S")


def _push(log_lines: list[str], line: str) -> None:
    log_lines.append(line)
    if len(log_lines) > MAX_LOG_LINES:
        del log_lines[: len(log_lines) - MAX_LOG_LINES]


def _render(
    mesh,
    log_lines: list[str],
    machine_name: str,
    mesh_mode: str,
    chat_id: str,
) -> None:
    # RustPython builds used by older xos binaries may not ship `shutil`,
    # so keep this renderer dependency-free and fixed-size for compatibility.
    width = DEFAULT_WIDTH
    height = DEFAULT_HEIGHT

    nodes = mesh.num_nodes()
    rank = mesh.rank()
    node_id = mesh.node_id()
    status = (
        " xos terminal "
        f"| channel={chat_id} mode={mesh_mode} "
        f"| nodes={nodes} rank={rank} "
        f"| machine={machine_name} id={_short_node_id(node_id)} "
    )

    # Keep bottom rows clear for the command prompt and typing area.
    log_height = max(3, height - 5)
    visible = log_lines[-log_height:]
    pad_count = log_height - len(visible)

    out = []
    out.append("\x1b[H")
    out.append(f"\x1b[7m{_fit(status, width)}\x1b[0m")
    out.append("-" * width)
    out.extend(_fit(line, width) for line in visible)
    out.extend(" " * width for _ in range(pad_count))
    out.append("-" * width)
    out.append(
        _fit("chat: type message + Enter  |  /quit exits terminal", width)
    )
    print("\n".join(out), end="", flush=True)


def _format_packet(packet) -> str:
    from_rank = getattr(packet, "from_rank", "?")
    from_id = getattr(packet, "from_id", "") or ""
    sender = getattr(packet, "sender", "") or _short_node_id(from_id)
    text = getattr(packet, "msg", "")
    return f"[{_stamp()}] [{sender} r{from_rank}] {text}"


def main() -> None:
    mesh = xos.mesh.connect(id=MESH_ID, mode=MODE)
    try:
        machine_name = mesh.node_name() or "machine"
    except Exception:
        machine_name = "machine"
    logs: list[str] = []
    _push(
        logs,
        (
            f"[{_stamp()}] joined global channel "
            f"id={MESH_ID!r} mode={MODE!r} as {machine_name}"
        ),
    )

    print("\x1b[?1049h\x1b[2J\x1b[H\x1b[?25l", end="", flush=True)

    last_render_at = 0.0
    try:
        while True:
            packets = mesh.receive(id="message", wait=False, latest_only=False)
            if packets:
                for packet in packets:
                    _push(logs, _format_packet(packet))

            line = xos.input("chat> ", wait=False)
            if line is not None:
                text = line.strip()
                if text in ("/quit", "/exit"):
                    _push(logs, f"[{_stamp()}] leaving xos terminal")
                    break
                if text:
                    mesh.broadcast(id="message", msg=text, sender=machine_name)
                    _push(logs, f"[{_stamp()}] [me] {text}")

            now = time.time()
            if (now - last_render_at) >= STATUS_REFRESH_SECS:
                _render(mesh, logs, machine_name, MODE, MESH_ID)
                last_render_at = now

            time.sleep(LOOP_SLEEP_SECS)
    finally:
        print("\x1b[?25h\x1b[?1049l", end="", flush=True)


main()
