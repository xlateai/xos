"""
LAN remote desktop viewer — pairs with the macOS daemon (`xos on`) on mesh ``xos-remote``.

Requirements: logged in on both sides (`xos login`), same LAN, daemon running on Mac.
Tap a desktop host row, control it from the viewport above the on-screen keyboard.
"""

import xos
import constants


class RemoteViewerApp(xos.Application):
    headless: bool = False

    def __init__(self):
        super().__init__()

        self.mesh = xos.mesh.connect(id=constants.MESH_CHANNEL, mode=constants.MODE)
        # self.status = xos.ui.text(
        #     "Connecting...",
        #     x1=0.0,
        #     y1=0.0,
        #     x2=1.0,
        #     y2=0.1,
        #     size=36.0,
        #     color=xos.color.CYAN,
        # )
        self.video = xos.ui.video(
            x1=0.0,
            y1=0.0,
            x2=1.0,
            y2=1.0,
        )

    def tick(self):
        # self.status.tick(self)
        # self.status.render(self)

        packet = self.mesh.receive(id="frame", wait=False, latest_only=True)
        if packet:
            frame = packet.frame
            # print(frame.tensor)   # repr uses one Rust reduction pass; Tensor.min/max/mean are native too
            self.video.set_frame(frame)
            print(self.t)
            # self.video.set_frame(frame.tensor)

        self.video.tick(self)

        # print(self.t)



if __name__ == "__main__":
    RemoteViewerApp().run()
