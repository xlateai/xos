import xos
import constants

# import ball
# BALL_APP = ball.BallDemo()

class RemoteSourceApp(xos.Application):
    headless: bool = True

    def __init__(self):
        super().__init__()

        self.mesh = constants.get_mesh()

    def tick(self):
        # print(self.t)
        # xos.device.get_device_frame()
        # BALL_APP.tick()
        # print(BALL_APP.frame)
        # self.mesh.broadcast(id="frame", frame=BALL_APP.frame)

        device_frame = xos.system.monitors[0].get_frame()
        # print(device_frame.tensor.shape)

        # if self.t % 10 == 0:
        self.mesh.broadcast(id="frame", frame=device_frame)
        # self.mesh.broadcast(id="frame", frame=device_frame)

        mouse_packets = self.mesh.receive(id="mouse", wait=False, latest_only=False)
        for packet in mouse_packets:
            xos.mouse.control(packet.mouse)


if __name__ == "__main__":
    RemoteSourceApp().run()
