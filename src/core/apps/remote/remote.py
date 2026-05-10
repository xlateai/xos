import xos
import constants
import viewer


class RemoteApp(xos.Application):
    headless: bool = False

    def __init__(self):
        super().__init__()

        self.mesh = constants.get_mesh()
        self.text = xos.ui.text(
            "",
            x1=0.0,
            y1=0.0,
            x2=1.0,
            y2=1.0,
            alignment=(0.0, 0.0),
            color=xos.color.WHITE,
            size=36.0,
        )


        # self.mesh = xos.mesh.connect(id=constants.MESH_CHANNEL, mode=constants.MODE, udp=constants.USE_UDP)

    def tick(self):
        self.text.text = f"num_nodes: {self.mesh.num_nodes()}"

        self.frame.clear(xos.color.BLACK)
        self.text.tick(self)
        self.text.render(self)

if __name__ == "__main__":
    # RemoteApp().run()
    viewer.RemoteViewerApp().run()