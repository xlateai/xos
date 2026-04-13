"""xos terminal: frame/tensor style renderer with text+color channels."""

import xos

MESH_ID = "terminal"
MODE = "lan"
LOOP_SLEEP_SECS = 0.05
MAX_LOG_LINES = 200
FOOTER_DEFAULT = "Type /help for help  |  /quit exits terminal"
FOOTER_XPY = "xpy mode active  |  /xpy exits python mode  |  /help shows commands"
FOOTER_HELP = [
    "Commands:",
    "  /help   show/hide this help",
    "  /nodes  list observed nodes by rank",
    "  /procs  list local xos-managed processes",
    "  /channels list channels seen on local managed procs",
    "  /xpy    toggle embedded Python REPL mode",
    "  /xos <args> run xos CLI command",
    "  /channel <id> switch channel (same mode)",
    "  /lan | /local | /online switch mesh mode",
    "  /clear  clear chat log",
    "  /quit   exit terminal",
]


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


def _identity_label(default_machine: str = "machine") -> str:
    username = ""
    node_name = default_machine
    try:
        username = (xos.auth.username() or "").strip()
    except Exception:
        username = ""
    try:
        node_name = (xos.auth.node_name() or "").strip() or default_machine
    except Exception:
        node_name = default_machine
    if username:
        return f"{username}@{node_name}"
    return node_name


def _safe_list_procs() -> list[dict]:
    try:
        return xos.manager.list_procs() or []
    except Exception:
        return []


def _proc_has_channel(proc: dict, channel_id: str, mode_u: str | None = None) -> bool:
    for ch in proc.get("channels", []) or []:
        cid = (ch.get("id", "") or "").strip()
        if cid != channel_id:
            continue
        if mode_u is None:
            return True
        cmode = (ch.get("mode", "") or "").upper()
        if cmode == mode_u:
            return True
    return False


def _is_local_daemon_active(procs: list[dict], local_node_id: str) -> bool:
    # Source of truth: daemon pid file + live pid check (same path used by `xos status`).
    try:
        if hasattr(xos.auth, "daemon_online"):
            return bool(xos.auth.daemon_online())
    except Exception:
        pass
    # Fallback for older binaries without the auth binding.
    for p in procs:
        if (p.get("label", "") or "") != "xos-daemon":
            continue
        if local_node_id and (p.get("node_id", "") or "") != local_node_id:
            continue
        return True
    return False


def _xpy_default_state(log_lines: list[str]) -> dict:
    def _xpy_print(*args, sep=" ", end="\n"):
        text = sep.join(str(a) for a in args)
        chunks = text.replace("\r\n", "\n").replace("\r", "\n").split("\n")
        emitted = False
        for chunk in chunks:
            chunk = chunk.strip("\r\n")
            if chunk.strip() == "":
                continue
            _append_log_line(log_lines, f"[🐍] {chunk}")
            emitted = True
        if not emitted and text.strip():
            _append_log_line(log_lines, f"[🐍] {text.strip()}")
        if end and end != "\n":
            suffix = end.rstrip()
            if suffix:
                _append_log_line(log_lines, f"[🐍] {suffix}")

    glb = {
        "__name__": "__xpy_console__",
        "__builtins__": __builtins__,
        "xos": xos,
        "print": _xpy_print,
    }
    return {"globals": glb, "buffer": []}


def _xpy_is_incomplete_syntax(err: SyntaxError) -> bool:
    text = str(err)
    needles = [
        "unexpected EOF",
        "incomplete input",
        "expected an indented block",
        "was never closed",
    ]
    return any(n in text for n in needles)


def _xpy_eval_line(log_lines: list[str], xpy_state: dict, line: str) -> None:
    buf = xpy_state["buffer"]
    glb = xpy_state["globals"]
    if buf or line.strip():
        buf.append(line)
    src = "\n".join(buf).rstrip("\n")
    if not src:
        return

    try:
        eval_code = compile(src, "<xpy>", "eval")
    except SyntaxError:
        eval_code = None
    except Exception as e:
        _append_log_line(log_lines, f"[🐍 err] {e}")
        buf.clear()
        return

    if eval_code is not None:
        try:
            value = eval(eval_code, glb, glb)
            if value is not None:
                _append_log_line(log_lines, f"[🐍] {value}")
        except Exception as e:
            _append_log_line(log_lines, f"[🐍 err] {e}")
        buf.clear()
        return

    try:
        exec_code = compile(src, "<xpy>", "exec")
    except SyntaxError as e:
        if _xpy_is_incomplete_syntax(e):
            return
        _append_log_line(log_lines, f"[🐍 err] {e}")
        buf.clear()
        return
    except Exception as e:
        _append_log_line(log_lines, f"[🐍 err] {e}")
        buf.clear()
        return

    try:
        exec(exec_code, glb, glb)
    except Exception as e:
        _append_log_line(log_lines, f"[🐍 err] {e}")
    buf.clear()


def _remember_node(known_nodes: dict, rank, node_id: str, label: str = "") -> None:
    try:
        r = int(rank)
    except Exception:
        return
    # A node can be re-ranked during failover; keep only its latest rank entry.
    if node_id:
        stale_ranks = []
        for existing_rank, existing in known_nodes.items():
            if existing_rank == r:
                continue
            if (existing.get("id", "") or "") == node_id:
                stale_ranks.append(existing_rank)
        for stale_rank in stale_ranks:
            known_nodes.pop(stale_rank, None)
    info = known_nodes.get(r, {})
    if node_id:
        info["id"] = node_id
    if label:
        info["label"] = label
    known_nodes[r] = info


def _emit_nodes(log_lines: list[str], known_nodes: dict, mesh) -> None:
    _append_log_line(log_lines, "nodes (rank order):")
    ranks = sorted(known_nodes.keys())
    if not ranks:
        _append_log_line(log_lines, "  `-- (none)")
    for i, rank in enumerate(ranks):
        branch = "`--" if i == len(ranks) - 1 else "|--"
        info = known_nodes.get(rank, {})
        nid = info.get("id", "?")
        label = info.get("label", "") or "unknown"
        _append_log_line(log_lines, f"  {branch} r{rank}: {label}  id={_short_node_id(nid)}")
    try:
        _append_log_line(log_lines, f"reported mesh.num_nodes()={mesh.num_nodes()}")
    except Exception:
        pass


def _emit_channels(log_lines: list[str], current_channel: str, current_mode: str, mesh=None) -> None:
    _append_log_line(log_lines, "channels (local managed processes):")
    procs = _safe_list_procs()

    channel_counts: dict[str, int] = {}
    channel_modes: dict[str, set[str]] = {}
    for p in procs:
        for ch in p.get("channels", []) or []:
            cid = (ch.get("id", "") or "").strip()
            if not cid:
                continue
            mode = (ch.get("mode", "local") or "local").upper()
            channel_counts[cid] = channel_counts.get(cid, 0) + 1
            channel_modes.setdefault(cid, set()).add(mode)

    # The active terminal channel should reflect the live mesh view immediately,
    # even if local process snapshots are briefly stale after rank failover.
    if mesh is not None:
        try:
            live = int(mesh.num_nodes())
            if live > 0:
                channel_counts[current_channel] = max(channel_counts.get(current_channel, 0), live)
                channel_modes.setdefault(current_channel, set()).add(current_mode.upper())
        except Exception:
            pass

    if not channel_counts:
        _append_log_line(log_lines, "  `-- (none)")
    else:
        cids = sorted(channel_counts.keys())
        for i, cid in enumerate(cids):
            branch = "`--" if i == len(cids) - 1 else "|--"
            marker = "*" if cid == current_channel else " "
            modes = ",".join(sorted(channel_modes.get(cid, {"LOCAL"})))
            count = channel_counts[cid]
            _append_log_line(
                log_lines,
                f"  {branch} [{marker}] {cid}  mode={modes}  procs={count}",
            )
    _append_log_line(
        log_lines,
        f"  +-- active: channel={current_channel!r} mode={current_mode.upper()}",
    )


def _emit_procs(log_lines: list[str]) -> None:
    _append_log_line(log_lines, "local managed processes:")
    procs = _safe_list_procs()
    if not procs:
        _append_log_line(log_lines, "  `-- (none)")
        return
    _append_log_line(log_lines, f"  +-- total: {len(procs)}")
    for i, p in enumerate(procs):
        branch = "`--" if i == len(procs) - 1 else "|--"
        rank = p.get("rank", "?")
        pid = p.get("pid", "?")
        label = p.get("label", "xos")
        node_id = p.get("node_id", "")
        _append_log_line(
            log_lines,
            f"  {branch} r{rank} pid={pid} {label} id={_short_node_id(node_id)}",
        )


def _emit_xos_cli(log_lines: list[str], argline: str) -> None:
    cmd = (argline or "").strip()
    if not cmd:
        _append_log_line(log_lines, "usage: /xos <cli args>  (example: /xos app whiteboard)")
        return
    if not hasattr(xos.manager, "run_xos"):
        _append_log_line(
            log_lines,
            "[xos err] this xos terminal runtime is missing run_xos; restart terminal after recompiling xos",
        )
        return
    try:
        res = xos.manager.run_xos(cmd)
    except Exception as e:
        _append_log_line(log_lines, f"[xos err] {e}")
        return

    shown = res.get("cmd", "")
    if shown:
        _append_log_line(log_lines, f"[xos] {shown}")
    if res.get("detached", False):
        pid = res.get("pid", "?")
        _append_log_line(log_lines, f"[xos] launched in background (pid={pid})")
        return pid

    code = int(res.get("code", -1) or -1)
    stdout = (res.get("stdout", "") or "").splitlines()
    stderr = (res.get("stderr", "") or "").splitlines()
    for line in stdout:
        _append_log_line(log_lines, f"[xos out] {line}")
    for line in stderr:
        _append_log_line(log_lines, f"[xos err] {line}")
    if not stdout and not stderr:
        _append_log_line(log_lines, f"[xos] exited with code {code}")
    return None


def _local_channel_nodes(channel_id: str, mode: str) -> dict[int, dict]:
    out: dict[int, dict] = {}
    mode_u = (mode or "").upper()
    procs = _safe_list_procs()
    for p in procs:
        pid = int(p.get("pid", 0) or 0)
        if pid <= 0:
            continue
        if not _proc_has_channel(p, channel_id, mode_u):
            continue
        out[pid] = {
            "rank": int(p.get("rank", -1) or -1),
            "label": p.get("label", "xos") or "xos",
            "node_id": p.get("node_id", "") or "",
        }
    return out


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


def _render(
    mesh,
    log_lines: list[str],
    machine_name: str,
    mesh_mode: str,
    chat_id: str,
    footer_lines: list[str],
    input_prompt: str = ">>> ",
) -> None:
    frame = xos.terminal.get_frame()
    width, height, channels = frame.shape

    mode_u = (mesh_mode or "").upper()
    procs = _safe_list_procs()
    try:
        terminal_count = int(mesh.num_nodes())
    except Exception:
        terminal_count = 1
    if terminal_count <= 0:
        terminal_count = 1
    process_count = terminal_count
    rank = mesh.rank()
    node_id = mesh.node_id()
    nodes = sum(1 for p in procs if _proc_has_channel(p, "global", mode_u))
    if nodes <= 0:
        nodes = 1
    daemon_active = _is_local_daemon_active(procs, node_id)
    daemon_icon = "●"
    daemon_word = "online" if daemon_active else "offline"
    daemon_color = "a" if daemon_active else "e"
    node_label = "node" if nodes == 1 else "nodes"
    terminal_label = "terminal" if terminal_count == 1 else "terminals"
    process_label = "process" if process_count == 1 else "processes"

    left = (
        f"{daemon_icon} {chat_id} | daemon {daemon_word} | {mesh_mode.upper()} | {nodes} {node_label} | "
        f"{terminal_count} {terminal_label} | {process_count} {process_label} | "
        f"term rank {rank} | id {_short_node_id(node_id)}"
    )
    right = machine_name
    min_gap = 2
    max_left = max(0, width - len(right) - min_gap)
    if len(left) > max_left:
        left = _fit(left, max_left).rstrip()
    gap = max(min_gap, width - len(left) - len(right))
    status = _fit(left + (" " * gap) + right, width)
    _put(frame, width, height, channels, 0, 0, status, "r")
    _put(frame, width, height, channels, 0, 0, daemon_icon, daemon_color)

    sep = "-" * width
    _put(frame, width, height, channels, 1, 0, sep, "8")

    log_start = 2
    prompt_row = max(0, height - 1)
    footer_count = max(1, len(footer_lines))
    footer_start = max(0, prompt_row - footer_count)
    bottom_sep = max(0, footer_start - 1)
    log_height = max(0, bottom_sep - log_start)
    visible = log_lines[-log_height:] if log_height > 0 else []
    for i, line in enumerate(visible):
        _put(
            frame,
            width,
            height,
            channels,
            log_start + i,
            0,
            _fit(line, width),
            _log_line_color(line),
        )

    _put(frame, width, height, channels, bottom_sep, 0, sep, "8")
    shown_footer = footer_lines[-footer_count:] if footer_lines else [FOOTER_DEFAULT]
    for i, line in enumerate(shown_footer):
        row = footer_start + i
        if row >= prompt_row:
            break
        _put(frame, width, height, channels, row, 0, _fit(line, width), "8")
    _put(frame, width, height, channels, prompt_row, 0, _fit(input_prompt, width), "b")
    try:
        xos.terminal.set_frame(frame, cursor_x=len(input_prompt), cursor_y=prompt_row)
    except Exception as e:
        # Terminal size can change between get_frame() and set_frame() calls.
        # Keep strict validation in Rust, but don't crash the UI loop on this race.
        if "terminal frame shape mismatch" not in str(e):
            raise


def _format_packet(packet) -> str:
    from_rank = getattr(packet, "from_rank", "?")
    from_id = getattr(packet, "from_id", "") or ""
    sender = getattr(packet, "sender", "") or _short_node_id(from_id)
    text = getattr(packet, "msg", "")
    return f"[{sender} r{from_rank}] {text}"


def _log_line_color(line: str) -> str:
    s = (line or "").strip()
    if not s:
        return "f"
    if s.startswith("channels (") or s.startswith("local managed processes:") or s.startswith("nodes ("):
        return "b"
    if s.startswith("[mesh]"):
        return "a"
    if s.startswith("[🐍 err]"):
        return "f"
    if s.startswith("[🐍]"):
        return "f"
    if s.startswith("[xos err]"):
        return "4"
    if s.startswith("[xos out]"):
        return "f"
    if s.startswith("[xos]"):
        return "a"
    if s.startswith("  +-- active:"):
        return "a"
    if "[*]" in s:
        return "a"
    if s.startswith("  +-- total:"):
        return "8"
    if s.startswith("  |--") or s.startswith("  `--"):
        return "f"
    return "f"


def main() -> None:
    current_channel = MESH_ID
    current_mode = MODE
    mesh = xos.mesh.connect(id=current_channel, mode=current_mode)
    machine_name = "machine"
    try:
        machine_name = mesh.node_name() or "machine"
    except Exception:
        pass
    display_name = _identity_label(machine_name)

    logs: list[str] = []
    help_expanded = False
    xpy_mode = False
    xpy_state = _xpy_default_state(logs)
    launched_pids: list[int] = []
    known_nodes: dict[int, dict[str, str]] = {}
    _remember_node(known_nodes, mesh.rank(), mesh.node_id(), display_name)
    _append_log_line(logs, f"joined {current_channel!r} in {current_mode.upper()} as {display_name}")

    print("\x1b[?1049h\x1b[2J\x1b[H", end="", flush=True)
    _render(
        mesh,
        logs,
        display_name,
        current_mode,
        current_channel,
        [FOOTER_DEFAULT],
        ">>> ",
    )
    last_size = (int(xos.terminal.get_width()), int(xos.terminal.get_height()))
    try:
        last_proc_version = int(xos.manager.version())
    except Exception:
        last_proc_version = 0
    try:
        last_mesh_nodes = int(mesh.num_nodes())
    except Exception:
        last_mesh_nodes = 1
    last_local_nodes = _local_channel_nodes(current_channel, current_mode)

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

            active_prompt = ">>> "
            active_footer = FOOTER_HELP if help_expanded else ([FOOTER_XPY] if xpy_mode else [FOOTER_DEFAULT])

            try:
                line = xos.input("", wait=False)
            except KeyboardInterrupt:
                # Interrupt priority: active xpy -> spawned child process -> terminal exit.
                if xpy_mode:
                    xpy_mode = False
                    xpy_state["buffer"].clear()
                    _append_log_line(logs, "🐍 xpy disabled (Ctrl+C)")
                    needs_render = True
                    line = None
                elif launched_pids:
                    pid = int(launched_pids.pop())
                    if hasattr(xos.manager, "kill_pid"):
                        try:
                            ok = bool(xos.manager.kill_pid(pid))
                            if ok:
                                _append_log_line(logs, f"[xos] stopped pid={pid} (Ctrl+C)")
                            else:
                                _append_log_line(logs, f"[xos err] could not stop pid={pid} (Ctrl+C)")
                        except Exception as e:
                            _append_log_line(logs, f"[xos err] kill_pid({pid}) failed: {e}")
                    else:
                        _append_log_line(logs, f"[xos err] kill_pid unavailable; cannot stop pid={pid}")
                    needs_render = True
                    line = None
                else:
                    _append_log_line(logs, "leaving xos terminal (Ctrl+C)")
                    _render(
                        mesh,
                        logs,
                        display_name,
                        current_mode,
                        current_channel,
                        active_footer,
                        active_prompt,
                    )
                    break
            if line is not None:
                text = line.strip()
                if text == "":
                    # Pure no-op; avoid unnecessary redraw flicker on blank Enter.
                    continue
                _remember_node(known_nodes, mesh.rank(), mesh.node_id(), display_name)
                if text in ("/quit", "/exit"):
                    _append_log_line(logs, "leaving xos terminal")
                    _render(
                        mesh,
                        logs,
                        display_name,
                        current_mode,
                        current_channel,
                        active_footer,
                        active_prompt,
                    )
                    break
                handled = False
                if text == "/help":
                    help_expanded = not help_expanded
                    needs_render = True
                    handled = True
                if text == "/xpy":
                    xpy_mode = not xpy_mode
                    if xpy_mode:
                        xpy_state = _xpy_default_state(logs)
                        _append_log_line(logs, "🐍 xpy enabled")
                    else:
                        xpy_state["buffer"].clear()
                        _append_log_line(logs, "🐍 xpy disabled")
                    needs_render = True
                    handled = True
                if text.startswith("/xos"):
                    argline = text[4:].strip()
                    spawned = _emit_xos_cli(logs, argline)
                    if isinstance(spawned, int) and spawned > 0:
                        launched_pids.append(spawned)
                    needs_render = True
                    handled = True
                if text == "/nodes":
                    _emit_nodes(logs, known_nodes, mesh)
                    needs_render = True
                    handled = True
                if text == "/procs":
                    _emit_procs(logs)
                    needs_render = True
                    handled = True
                if text == "/channels":
                    _emit_channels(logs, current_channel, current_mode, mesh)
                    needs_render = True
                    handled = True
                if text == "/clear":
                    logs.clear()
                    needs_render = True
                    handled = True
                if text.startswith("/channel "):
                    next_channel = text.split(None, 1)[1].strip()
                    if not next_channel:
                        _append_log_line(logs, "usage: /channel <id>")
                    else:
                        try:
                            next_mesh = xos.mesh.connect(id=next_channel, mode=current_mode)
                            mesh = next_mesh
                            current_channel = next_channel
                            display_name = _identity_label(machine_name)
                            known_nodes.clear()
                            _remember_node(known_nodes, mesh.rank(), mesh.node_id(), display_name)
                            _append_log_line(logs, f"switched to channel {current_channel!r} ({current_mode.upper()})")
                        except Exception as e:
                            _append_log_line(logs, f"channel switch failed: {e}")
                    needs_render = True
                    handled = True
                if text in ("/lan", "/local", "/online"):
                    next_mode = text[1:]
                    try:
                        next_mesh = xos.mesh.connect(id=current_channel, mode=next_mode)
                        mesh = next_mesh
                        current_mode = next_mode
                        display_name = _identity_label(machine_name)
                        known_nodes.clear()
                        _remember_node(known_nodes, mesh.rank(), mesh.node_id(), display_name)
                        _append_log_line(logs, f"switched to {current_mode.upper()} on {current_channel!r}")
                    except Exception as e:
                        _append_log_line(logs, f"mode switch failed: {e}")
                    needs_render = True
                    handled = True
                if not handled and (text or (xpy_mode and xpy_state["buffer"])):
                    if xpy_mode:
                        if text in ("exit()", "quit()"):
                            xpy_mode = False
                            xpy_state["buffer"].clear()
                            _append_log_line(logs, "🐍 xpy disabled")
                        else:
                            _xpy_eval_line(logs, xpy_state, line.rstrip("\n"))
                    else:
                        mesh.broadcast(id="message", msg=text, sender=display_name)
                        _append_log_line(logs, f"[me] {text}")
                    needs_render = True

            size_now = (int(xos.terminal.get_width()), int(xos.terminal.get_height()))
            if size_now != last_size:
                last_size = size_now
                needs_render = True
            try:
                mesh_nodes = int(mesh.num_nodes())
            except Exception:
                mesh_nodes = last_mesh_nodes
            if mesh_nodes != last_mesh_nodes:
                local_nodes_now = _local_channel_nodes(current_channel, current_mode)
                joined_pids = sorted(set(local_nodes_now.keys()) - set(last_local_nodes.keys()))
                left_pids = sorted(set(last_local_nodes.keys()) - set(local_nodes_now.keys()))
                detailed_events = 0
                for pid in joined_pids:
                    info = local_nodes_now.get(pid, {})
                    _append_log_line(
                        logs,
                        "[mesh] joined "
                        f"r{info.get('rank', '?')} {info.get('label', 'xos')} "
                        f"id={_short_node_id(info.get('node_id', ''))} pid={pid}",
                    )
                    detailed_events += 1
                for pid in left_pids:
                    info = last_local_nodes.get(pid, {})
                    _append_log_line(
                        logs,
                        "[mesh] left "
                        f"r{info.get('rank', '?')} {info.get('label', 'xos')} "
                        f"id={_short_node_id(info.get('node_id', ''))} pid={pid}",
                    )
                    detailed_events += 1

                if mesh_nodes > last_mesh_nodes:
                    delta = mesh_nodes - last_mesh_nodes
                    remaining = max(0, delta - detailed_events)
                    if remaining > 0:
                        noun = "node" if remaining == 1 else "nodes"
                        _append_log_line(logs, f"[mesh] +{remaining} {noun} joined (remote/unknown) (now {mesh_nodes})")
                else:
                    delta = last_mesh_nodes - mesh_nodes
                    remaining = max(0, delta - detailed_events)
                    if remaining > 0:
                        noun = "node" if remaining == 1 else "nodes"
                        _append_log_line(logs, f"[mesh] -{remaining} {noun} left (remote/unknown) (now {mesh_nodes})")

                last_mesh_nodes = mesh_nodes
                last_local_nodes = local_nodes_now
                needs_render = True
            try:
                proc_version = int(xos.manager.version())
            except Exception:
                proc_version = last_proc_version
            if proc_version != last_proc_version:
                last_proc_version = proc_version
                local_nodes_now = _local_channel_nodes(current_channel, current_mode)
                if local_nodes_now != last_local_nodes:
                    last_local_nodes = local_nodes_now
                    needs_render = True

            if needs_render:
                active_prompt = ">>> "
                active_footer = FOOTER_HELP if help_expanded else ([FOOTER_XPY] if xpy_mode else [FOOTER_DEFAULT])
                _render(
                    mesh,
                    logs,
                    display_name,
                    current_mode,
                    current_channel,
                    active_footer,
                    active_prompt,
                )

            xos.sleep(LOOP_SLEEP_SECS)
    finally:
        print("\x1b[?1049l", end="", flush=True)


main()
