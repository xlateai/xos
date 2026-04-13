"""xos status: compact mesh/channels/procs snapshot."""

import xos


def _short_node_id(node_id: str) -> str:
    if not node_id:
        return "?"
    return node_id[:12] + ("..." if len(node_id) > 12 else "")


def _proc_row(proc: dict) -> dict:
    return {
        "pid": int(proc.get("pid", 0) or 0),
        "rank": int(proc.get("rank", -1) or -1),
        "label": proc.get("label", "xos") or "xos",
        "node_id": proc.get("node_id", "") or "",
        "channels": proc.get("channels", []) or [],
    }


def _channel_index(rows: list[dict]) -> dict[str, dict]:
    by_channel: dict[str, dict] = {}
    for row in rows:
        for ch in row["channels"]:
            cid = (ch.get("id", "") or "").strip()
            if not cid:
                continue
            mode = (ch.get("mode", "local") or "local").upper()
            entry = by_channel.setdefault(cid, {"modes": set(), "rows": []})
            entry["modes"].add(mode)
            if not any(r["pid"] == row["pid"] for r in entry["rows"]):
                entry["rows"].append(row)
    return by_channel


def _count_channel(by_channel: dict, channel_id: str) -> int:
    return len(by_channel.get(channel_id, {}).get("rows", []))


def _collect_procs_with_brief_settle() -> list[dict]:
    # Manager hello cadence is ~350ms; brief settle avoids "status sees only itself".
    best: list[dict] = xos.manager.list_procs() or []
    for _ in range(6):
        xos.sleep(0.12)
        current = xos.manager.list_procs() or []
        if len(current) > len(best):
            best = current
    return best


def main() -> None:
    rows = sorted((_proc_row(p) for p in _collect_procs_with_brief_settle()), key=lambda r: (r["rank"], r["pid"]))
    by_channel = _channel_index(rows)

    machines = max(1, _count_channel(by_channel, "global"))
    terminals = max(1, _count_channel(by_channel, "terminal"))
    processes = len(rows)

    machine_label = "machine" if machines == 1 else "machines"
    terminal_label = "terminal" if terminals == 1 else "terminals"
    process_label = "process" if processes == 1 else "processes"

    lines = [f"o status | {machines} {machine_label} | {terminals} {terminal_label} | {processes} {process_label}"]
    if by_channel:
        parts = [f"{cid}={len(by_channel[cid]['rows'])}" for cid in sorted(by_channel.keys())]
        lines.append("channels: " + " | ".join(parts))
    else:
        lines.append("channels: (none)")

    if not rows:
        lines.append("local managed processes: (none)")
    else:
        lines.append("local managed processes:")
        for row in rows:
            lines.append(
                f"- r{row['rank']} pid={row['pid']} {row['label']} "
                f"id={_short_node_id(row['node_id'])}"
            )

    print("\n".join(lines))


main()
