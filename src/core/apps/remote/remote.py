"""
LAN remote desktop viewer — pairs with the macOS daemon (`xos on`) on mesh ``xos-remote``.

Requirements: logged in on both sides (`xos login`), same LAN, daemon running on Mac.
Tap a desktop host row, control it from the viewport above the on-screen keyboard.
"""

import xos


MESH_CHANNEL = "remote"
MODE = "lan"


class RemoteApp(xos.Application):

    def __init__(self):
        self.mesh = xos.mesh.connect(id=MESH_CHANNEL, mode=MODE)
        self.video = xos.ui.video(
            x1=0.0,
            y1=0.0,
            x2=1.0,
            y2=1.0,
            video=self.mesh.receive(id="video"),
        )

    def tick(self):

        while True:
            # Drain inbound first so chat updates without waiting for local input.
            packets = mesh.receive(id="message", wait=False, latest_only=False)


if __name__ == "__main__":
    RemoteApp().run()
