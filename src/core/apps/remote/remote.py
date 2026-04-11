"""
Remote desktop over mesh — Python entrypoint for `xpy path/to/remote.py`.

- **Rank 0 / viewer**: composites ``remote_frame`` packets each tick; pointer input is sent on mouse
  events (and scroll via ``on_scroll`` / tick coalescing).
- **Rank 1 / streamer**: applies inbound ``remote_input`` via ``xos.mouse.apply_remote_input``; sends
  desktop JPEG frames with ``mesh.send(..., stream_frame=self.screen)`` (kernel captures on send).

Run the viewer first, then the streamer. LAN mode expects ``xos login --offline``.
"""

from __future__ import annotations

import time
from typing import Any, Dict, List, Optional

import xos

MESH_ID = "xos-remote"
MODE = "lan"

KIND_FRAME = "remote_frame"
KIND_INPUT = "remote_input"

VIEWER_RANK = 0
STREAMER_RANK = 1

mesh = xos.mesh.connect(id=MESH_ID, mode=MODE)
RANK = mesh.rank()


def _norm_pointer(x: float, y: float, width: int, height: int) -> tuple[float, float]:
    fw = max(float(width), 1.0)
    fh = max(float(height), 1.0)
    return (x / fw, y / fh)


class RemoteViewer(xos.Application):
    """Rank 0: show streamer frames; send input on pointer events."""

    def __init__(self) -> None:
        super().__init__(headless=False)
        self._pending_scroll = 0.0

    def tick(self) -> None:
        stream_frame = mesh.receive(id=KIND_FRAME, wait=False, latest_only=True)
        if stream_frame:
            xos.rasterizer.frame_in_frame(self.frame, stream_frame)

        if self._pending_scroll:
            self._push_input()
            self._pending_scroll = 0.0

    def _push_input(self) -> None:
        w, h = self.frame.get_width(), self.frame.get_height()
        nx, ny = _norm_pointer(float(self.mouse["x"]), float(self.mouse["y"]), w, h)
        mesh.send(
            id=KIND_INPUT,
            to=STREAMER_RANK,
            nx=nx,
            ny=ny,
            left=bool(self.mouse["is_left_clicking"]),
            right=bool(self.mouse.get("is_right_clicking", False)),
            scroll=float(self._pending_scroll),
        )

    def on_mouse_move(self, x: float, y: float) -> None:
        self._push_input()

    def on_mouse_down(self, x: float, y: float) -> None:
        self._push_input()

    def on_mouse_up(self, x: float, y: float) -> None:
        self._push_input()

    def on_scroll(self, dx: float, dy: float) -> None:
        self._pending_scroll += dy
        self._push_input()


class RemoteStreamer(xos.Application):
    """Rank 1: apply remote input; send frames at most ``fps`` times per second (default 30)."""

    def __init__(self, fps: float = 30.0) -> None:
        super().__init__(headless=True)
        self._fps = max(fps, 1e-3)
        self._min_frame_interval = 1.0 / self._fps
        self._last_frame_sent_at: Optional[float] = None

    def tick(self) -> None:
        self._apply_remote_input()

        now = time.perf_counter()
        if self._last_frame_sent_at is not None:
            if (now - self._last_frame_sent_at) < self._min_frame_interval:
                return

        mesh.send(id=KIND_FRAME, to=VIEWER_RANK, stream_frame=self.screen)
        self._last_frame_sent_at = now

    def _apply_remote_input(self) -> None:
        packets = mesh.receive(id=KIND_INPUT, wait=False, latest_only=False)
        if not packets:
            return
        payload = _coalesce_input(packets)
        if not payload:
            return
        xos.mouse.apply_remote_input(payload)


def _coalesce_input(packets: List[Any]) -> Optional[Dict[str, Any]]:
    """Last sample wins for position and buttons; scroll sums."""
    if not packets:
        return None
    last = packets[-1]
    scroll_total = 0.0
    for p in packets:
        scroll_total += float(getattr(p, "scroll", 0.0) or 0.0)
    return {
        "nx": float(getattr(last, "nx", 0.5)),
        "ny": float(getattr(last, "ny", 0.5)),
        "left": bool(getattr(last, "left", False)),
        "right": bool(getattr(last, "right", False)),
        "scroll": scroll_total,
    }


def main() -> None:
    if RANK not in (VIEWER_RANK, STREAMER_RANK):
        raise RuntimeError(
            f"remote app expects mesh rank {VIEWER_RANK} (viewer) or {STREAMER_RANK} (streamer); got {RANK}"
        )
    if RANK == VIEWER_RANK:
        RemoteViewer().run()
    else:
        RemoteStreamer().run()


if __name__ == "__main__":
    main()

__all__ = [
    "KIND_FRAME",
    "KIND_INPUT",
    "MESH_ID",
    "MODE",
    "RANK",
    "RemoteStreamer",
    "RemoteViewer",
    "STREAMER_RANK",
    "VIEWER_RANK",
    "main",
    "mesh",
]
