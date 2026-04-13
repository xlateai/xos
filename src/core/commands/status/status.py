"""xos status: lightweight mesh/channels/procs snapshot."""

import xos

TERMINAL_CHANNEL = "terminal"
TERMINAL_MODE = "lan"


def _cp(text: str) -> None:
    xos.color_print(text)


def _short_node_id(node_id: str) -> str:
    if not node_id:
        return "?"
    if len(node_id) <= 12:
        return node_id
    return node_id[:12] + "..."


def _print_machines() -> None:
    _cp("&bmachines (terminal mesh):&f")
    try:
        mesh = xos.mesh.connect(id=TERMINAL_CHANNEL, mode=TERMINAL_MODE)
        machines = int(mesh.num_nodes())
        noun = "machine" if machines == 1 else "machines"
        _cp(f"  &8+--&f total: {machines} {noun}")
        _cp(
            "  &8`--&f "
            f"channel='{TERMINAL_CHANNEL}' mode={TERMINAL_MODE.upper()} "
            f"rank={mesh.rank()} id={_short_node_id(mesh.node_id())}"
        )
    except Exception as e:
        _cp(f"  &8`--&c unavailable: {e}")


def _proc_row(proc: dict) -> dict:
    return {
        "pid": int(proc.get("pid", 0) or 0),
        "rank": int(proc.get("rank", -1) or -1),
        "label": proc.get("label", "xos") or "xos",
        "node_id": proc.get("node_id", "") or "",
    }


def _print_channels(procs: list) -> None:
    _cp("&bchannels (local managed processes):&f")
    by_channel = {}
    for proc in procs:
        row = _proc_row(proc)
        for ch in proc.get("channels", []) or []:
            cid = (ch.get("id", "") or "").strip()
            if not cid:
                continue
            mode = (ch.get("mode", "local") or "local").upper()
            entry = by_channel.setdefault(cid, {"modes": set(), "rows": []})
            entry["modes"].add(mode)
            if not any(int(r.get("pid", 0) or 0) == row["pid"] for r in entry["rows"]):
                entry["rows"].append(row)

    channel_ids = sorted(by_channel.keys())
    if not channel_ids:
        _cp("  &8`--&f (none)")
        return

    for i, cid in enumerate(channel_ids):
        entry = by_channel[cid]
        rows = sorted(entry["rows"], key=lambda r: (r["rank"], r["pid"]))
        modes = ",".join(sorted(entry["modes"]))
        branch = "`--" if i == len(channel_ids) - 1 else "|--"
        _cp(f"  &8{branch}&f {cid}  mode={modes}  procs={len(rows)}")
        for j, row in enumerate(rows):
            pbranch = "`--" if j == len(rows) - 1 else "|--"
            _cp(
                "      "
                f"&8{pbranch}&f r{row['rank']} pid={row['pid']} "
                f"{row['label']} id={_short_node_id(row['node_id'])}"
            )


def _print_procs(procs: list) -> None:
    _cp("&blocal managed processes:&f")
    if not procs:
        _cp("  &8`--&f (none)")
        return
    _cp(f"  &8+--&f total: {len(procs)}")
    rows = sorted((_proc_row(p) for p in procs), key=lambda r: (r["rank"], r["pid"]))
    for i, row in enumerate(rows):
        branch = "`--" if i == len(rows) - 1 else "|--"
        _cp(
            f"  &8{branch}&f r{row['rank']} pid={row['pid']} "
            f"{row['label']} id={_short_node_id(row['node_id'])}"
        )


def main() -> None:
    procs = xos.manager.list_procs() or []
    _print_machines()
    _print_channels(procs)
    _print_procs(procs)


main()
