"""
Remote desktop over mesh — Python entrypoint for `xpy path/to/remote.py`.

- **Rank 0 / viewer**: composites ``remote_frame`` packets each tick; pointer input is sent on mouse
  events (and scroll via ``on_scroll`` / tick coalescing).
- **Rank 1 / streamer**: applies inbound ``remote_input`` via ``xos.mouse.apply_remote_input``; sends
  desktop JPEG frames with ``mesh.send(..., stream_frame=self.screen)`` (kernel captures on send).

Run the viewer first, then the streamer. LAN mode expects ``xos login --offline``.
"""

import xos

MESH_ID = "xos-remote"
MODE = "lan"

KIND_FRAME = "remote_frame"
KIND_INPUT = "remote_input"

VIEWER_RANK = 0
STREAMER_RANK = 1

mesh = xos.mesh.connect(id=MESH_ID, mode=MODE)
RANK = mesh.rank()

print("Rank:", RANK)


def _norm_pointer(x, y, width, height):
    fw = max(float(width), 1.0)
    fh = max(float(height), 1.0)
    return (x / fw, y / fh)


class RemoteViewer(xos.Application):
    """Rank 0: show streamer frames; send input on pointer events."""

    def __init__(self):
        super().__init__(headless=False)
        self._pending_scroll = 0.0

    def tick(self):
        stream_frame = mesh.receive(id=KIND_FRAME, wait=False, latest_only=True)
        if stream_frame:
            xos.rasterizer.frame_in_frame(self.frame, stream_frame)

        if self._pending_scroll:
            self._push_input()
            self._pending_scroll = 0.0

    def _push_input(self):
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

    def on_mouse_move(self, x, y):
        self._push_input()

    def on_mouse_down(self, x, y):
        self._push_input()

    def on_mouse_up(self, x, y):
        self._push_input()

    def on_scroll(self, dx, dy):
        self._pending_scroll += dy
        self._push_input()


class RemoteStreamer(xos.Application):
    """Rank 1: apply remote input; send frames at most ``fps`` times per second (default 30)."""

    def __init__(self, fps=60.0):
        super().__init__(headless=True)
        self._fps = max(fps, 1e-3)
        self._min_frame_interval = 1.0 / self._fps
        self._time_until_send = 0.0

    def tick(self):
        self._apply_remote_input()
        self._time_until_send -= float(getattr(self, "dt", 0.0) or 0.0)
        if self._time_until_send > 0.0:
            return

        mesh.send(id=KIND_FRAME, to=VIEWER_RANK, stream_frame=self.screen)
        self._time_until_send = self._min_frame_interval

    def _apply_remote_input(self):
        packets = mesh.receive(id=KIND_INPUT, wait=False, latest_only=False)
        if not packets:
            return
        # Preserve press/release transitions by applying each packet in order.
        for packet in packets:
            xos.mouse.apply_remote_input(
                {
                    "nx": float(getattr(packet, "nx", 0.5)),
                    "ny": float(getattr(packet, "ny", 0.5)),
                    "left": bool(getattr(packet, "left", False)),
                    "right": bool(getattr(packet, "right", False)),
                    "scroll": float(getattr(packet, "scroll", 0.0) or 0.0),
                }
            )


def main():
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
