
import xos


class ViewportTesting(xos.Application):

    headless: bool = False

    def __init__(self):
        super().__init__()

    def tick(self):
        self.frame.clear(xos.color.BLACK)

if __name__ == "__main__":
    viewport1 = ViewportTesting()
    viewport2 = ViewportTesting()
    
    for i in range(10000):
        viewport1.tick()
        viewport2.tick()