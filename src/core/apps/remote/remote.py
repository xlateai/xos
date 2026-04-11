"""
Remote desktop over mesh — Python shape of the app (native kernel will match this).

- **Rank 0 / viewer**: windowed, composites the streamer's frames each tick; pointer input is sent
  on mouse *events* (move / down / up), not duplicated every tick.
- **Rank 1 / streamer**: headless capture; applies inbound input; sends frames at most ``fps`` times
  per second (default 30), paced inside ``tick()`` via a wall-clock interval.

Run the viewer first, then the streamer (same ``MESH_ID``, two nodes). LAN mode expects
``xos login --offline`` for identity.
"""

from __future__ import annotations

import time
from typing import Any, List, Optional

import xos

# --- session (must stay aligned with the Rust remote kernel) ---
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


def _peer_norm_to_desktop_pixels(nx: float, ny: float) -> tuple[float, float]:
    """Map viewer-normalized pointer into streamer desktop pixels (native code owns real bounds)."""
    # Placeholder until the kernel threads virtual-screen geometry into this path.
    return (nx * 1920.0, ny * 1080.0)


class RemoteViewer(xos.Application):
    """
    Viewer: pull the latest remote frame each tick; push input to the streamer on pointer events.

    Scroll deltas are coalesced and flushed from ``tick()`` until wheel events are forwarded into
    Python (then ``on_scroll`` can take over).
    """

    def __init__(self) -> None:
        super().__init__(headless=False)
        self._pending_scroll = 0.0

    def tick(self) -> None:
        packet = mesh.receive(id=KIND_FRAME, wait=False, latest_only=True)
        if packet is not None:
            src = getattr(packet, "stream_frame", None) or packet
            xos.rasterizer.frame_in_frame(self.frame, src)

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
        """When the engine wires wheel events here, scroll becomes fully event-driven too."""
        self._pending_scroll += dy
        self._push_input()


class RemoteStreamer(xos.Application):
    """
    Streamer: drain merged input, then send at most one frame per ``1/fps`` seconds (default 30 fps).
    """

    def __init__(self, fps: float = 30.0) -> None:
        super().__init__(headless=True)
        self._fps = max(fps, 1e-3)
        self._min_frame_interval = 1.0 / self._fps
        self._last_frame_sent_at: Optional[float] = None
        self._prev_left = False
        self._prev_right = False

    def tick(self) -> None:
        self._apply_remote_input()

        now = time.perf_counter()
        if self._last_frame_sent_at is not None:
            if (now - self._last_frame_sent_at) < self._min_frame_interval:
                return

        # Kernel passes capture without an extra Python copy (screen / capture handle).
        mesh.send(id=KIND_FRAME, to=VIEWER_RANK, stream_frame=self.screen)
        self._last_frame_sent_at = now

    def _apply_remote_input(self) -> None:
        packets = mesh.receive(id=KIND_INPUT, wait=False, latest_only=False)
        if not packets:
            return
        merged = _coalesce_input(packets)
        if merged is None:
            return
        if xos.mouse is None:
            raise RuntimeError("streamer node needs a system mouse sink (xos.mouse)")
        px, py = _peer_norm_to_desktop_pixels(merged.nx, merged.ny)
        xos.mouse.set_position(px, py)
        if merged.left_down and not self._prev_left:
            xos.mouse.left_click()
        if merged.right_down and not self._prev_right:
            xos.mouse.right_click()
        self._prev_left = merged.left_down
        self._prev_right = merged.right_down
        if merged.scroll:
            xos.mouse.scroll_y(merged.scroll)


class _MergedPointer:
    __slots__ = ("nx", "ny", "left_down", "right_down", "scroll")

    def __init__(
        self,
        nx: float,
        ny: float,
        *,
        left_down: bool,
        right_down: bool,
        scroll: float,
    ) -> None:
        self.nx = nx
        self.ny = ny
        self.left_down = left_down
        self.right_down = right_down
        self.scroll = scroll


def _coalesce_input(packets: List[Any]) -> Optional[_MergedPointer]:
    """
    Last sample wins for position and buttons; scroll values sum (matches the native coalescer).
    """
    if not packets:
        return None
    last = packets[-1]
    scroll_total = 0.0
    for p in packets:
        scroll_total += float(getattr(p, "scroll", 0.0) or 0.0)

    nx = float(getattr(last, "nx", 0.5))
    ny = float(getattr(last, "ny", 0.5))

    return _MergedPointer(
        nx,
        ny,
        left_down=bool(getattr(last, "left", False)),
        right_down=bool(getattr(last, "right", False)),
        scroll=scroll_total,
    )


def main() -> None:
    if RANK not in (VIEWER_RANK, STREAMER_RANK):
        raise RuntimeError(
            f"remote app expects mesh rank {VIEWER_RANK} (viewer) or {STREAMER_RANK} (streamer); got {RANK}"
        )
    if RANK == VIEWER_RANK:
        RemoteViewer().run()
    else:
        RemoteStreamer().run()


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
