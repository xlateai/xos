"""xos terminal: frame/tensor style renderer with text+color channels."""

import xos

MESH_ID = "xos-global"
MODE = "lan"
LOOP_SLEEP_SECS = 0.05
MAX_LOG_LINES = 200


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


def _append_log_line(log_lines: list[str], line: str) -> None:
    log_lines.append(line)
    if len(log_lines) > MAX_LOG_LINES:
        del log_lines[: len(log_lines) - MAX_LOG_LINES]


def _remember_node(known_nodes: dict, rank, node_id: str, label: str = "") -> None:
    try:
        r = int(rank)
    except Exception:
        return
    info = known_nodes.get(r, {})
    if node_id:
        info["id"] = node_id
    if label:
        info["label"] = label
    known_nodes[r] = info


def _emit_nodes(log_lines: list[str], known_nodes: dict, mesh) -> None:
    _append_log_line(log_lines, "nodes (rank order):")
    for rank in sorted(known_nodes.keys()):
        info = known_nodes.get(rank, {})
        nid = info.get("id", "?")
        label = info.get("label", "") or "unknown"
        _append_log_line(log_lines, f"  r{rank}: {label}  id={_short_node_id(nid)}")
    try:
        _append_log_line(log_lines, f"reported mesh.num_nodes()={mesh.num_nodes()}")
    except Exception:
        pass


def _idx(x: int, y: int, ch: int, width: int, height: int, channels: int) -> int:
    return ((x * height + y) * channels) + ch


def _put(frame, width: int, height: int, channels: int, row: int, col: int, text: str, color: str = "f") -> None:
    if row < 0 or row >= height or col >= width or channels < 2:
        return
    flat = frame._data["_data"]
    for i, ch in enumerate(text):
        x = col + i
        if x >= width:
            break
        flat[_idx(x, row, 0, width, height, channels)] = ch
        flat[_idx(x, row, 1, width, height, channels)] = color


def _render(mesh, log_lines: list[str], machine_name: str, mesh_mode: str, chat_id: str) -> None:
    frame = xos.terminal.get_frame()
    width, height, channels = frame.shape

    nodes = mesh.num_nodes()
    rank = mesh.rank()
    node_id = mesh.node_id()
    node_label = "node" if nodes == 1 else "nodes"

    left = (
        f"o {chat_id} | {mesh_mode.upper()} | {nodes} {node_label} | "
        f"rank {rank} | id {_short_node_id(node_id)}"
    )
    right = machine_name
    min_gap = 2
    max_left = max(0, width - len(right) - min_gap)
    if len(left) > max_left:
        left = _fit(left, max_left).rstrip()
    gap = max(min_gap, width - len(left) - len(right))
    status = _fit(left + (" " * gap) + right, width)
    _put(frame, width, height, channels, 0, 0, status, "r")
    _put(frame, width, height, channels, 0, 0, "o", "a")

    sep = "-" * width
    _put(frame, width, height, channels, 1, 0, sep, "8")

    log_start = 2
    prompt_row = max(0, height - 1)
    help_row = max(0, height - 2)
    bottom_sep = max(0, height - 3)
    log_height = max(0, bottom_sep - log_start)
    visible = log_lines[-log_height:] if log_height > 0 else []
    for i, line in enumerate(visible):
        _put(frame, width, height, channels, log_start + i, 0, _fit(line, width), "f")

    _put(frame, width, height, channels, bottom_sep, 0, sep, "8")
    _put(frame, width, height, channels, help_row, 0, _fit("Type message + Enter  |  /quit exits terminal", width), "8")
    _put(frame, width, height, channels, prompt_row, 0, _fit(">>> ", width), "b")
    xos.terminal.set_frame(frame, cursor_x=4, cursor_y=prompt_row)


def _format_packet(packet) -> str:
    from_rank = getattr(packet, "from_rank", "?")
    from_id = getattr(packet, "from_id", "") or ""
    sender = getattr(packet, "sender", "") or _short_node_id(from_id)
    text = getattr(packet, "msg", "")
    return f"[{sender} r{from_rank}] {text}"


def main() -> None:
    mesh = xos.mesh.connect(id=MESH_ID, mode=MODE)
    machine_name = "machine"
    try:
        machine_name = mesh.node_name() or "machine"
    except Exception:
        pass

    logs: list[str] = []
    known_nodes: dict[int, dict[str, str]] = {}
    _remember_node(known_nodes, mesh.rank(), mesh.node_id(), machine_name)
    _append_log_line(logs, f"joined {MESH_ID!r} in {MODE.upper()} as {machine_name}")

    print("\x1b[?1049h\x1b[2J\x1b[H", end="", flush=True)
    _render(mesh, logs, machine_name, MODE, MESH_ID)
    last_size = (int(xos.terminal.get_width()), int(xos.terminal.get_height()))

    try:
        while True:
            needs_render = False

            packets = mesh.receive(id="message", wait=False, latest_only=False)
            if packets:
                for packet in packets:
                    _remember_node(
                        known_nodes,
                        getattr(packet, "from_rank", None),
                        getattr(packet, "from_id", "") or "",
                        getattr(packet, "sender", "") or "",
                    )
                    _append_log_line(logs, _format_packet(packet))
                needs_render = True

            line = xos.input("", wait=False)
            if line is not None:
                text = line.strip()
                _remember_node(known_nodes, mesh.rank(), mesh.node_id(), machine_name)
                if text in ("/quit", "/exit"):
                    _append_log_line(logs, "leaving xos terminal")
                    _render(mesh, logs, machine_name, MODE, MESH_ID)
                    break
                if text == "/nodes":
                    _emit_nodes(logs, known_nodes, mesh)
                    needs_render = True
                    continue
                if text:
                    mesh.broadcast(id="message", msg=text, sender=machine_name)
                    _append_log_line(logs, f"[me] {text}")
                    needs_render = True

            size_now = (int(xos.terminal.get_width()), int(xos.terminal.get_height()))
            if size_now != last_size:
                last_size = size_now
                needs_render = True

            if needs_render:
                _render(mesh, logs, machine_name, MODE, MESH_ID)

            xos.sleep(LOOP_SLEEP_SECS)
    finally:
        print("\x1b[?1049l", end="", flush=True)


main()
