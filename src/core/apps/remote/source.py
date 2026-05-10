import xos
import constants

# import ball
# BALL_APP = ball.BallDemo()

class RemoteSourceApp(xos.Application):
    headless: bool = True

    def __init__(self):
        super().__init__()

        self.mesh = xos.mesh.connect(id=constants.MESH_CHANNEL, mode=constants.MODE, udp=constants.USE_UDP)

    def tick(self):
        print(self.t)
        # xos.device.get_device_frame()
        # BALL_APP.tick()
        # print(BALL_APP.frame)
        # self.mesh.broadcast(id="frame", frame=BALL_APP.frame)

        device_frame = xos.system.monitors[0].get_frame()
        print(device_frame.tensor)
        # self.mesh.broadcast(id="frame", frame=device_frame)


if __name__ == "__main__":
    RemoteSourceApp().run()
