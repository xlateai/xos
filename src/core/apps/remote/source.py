import xos
import utils

import ball

BALL_APP = ball.BallDemo()

class RemoteSourceApp(xos.Application):
    headless: bool = True

    def __init__(self):
        super().__init__()

        self.mesh = xos.mesh.connect(id=utils.MESH_CHANNEL, mode=utils.MODE)

    def tick(self):
        print(self.t)
        # xos.device.get_device_frame()

        self.mesh.broadcast(id="frame", frame="test")

        BALL_APP.tick()


if __name__ == "__main__":
    RemoteSourceApp().run()
