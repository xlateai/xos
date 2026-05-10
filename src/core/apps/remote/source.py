import xos
import utils


class RemoteSourceApp(xos.Application):
    headless: bool = True

    def __init__(self):
        self.mesh = xos.mesh.connect(id=MESH_CHANNEL, mode=MODE)

    def tick(self):
        packet = self.mesh.receive(id="frame", wait=False, latest_only=False)


if __name__ == "__main__":
    RemoteViewerApp().run()
