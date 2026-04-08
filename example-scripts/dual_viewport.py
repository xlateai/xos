
import xos


class ViewportTesting(xos.Application):

    headless: bool = False

    def __init__(self):
        super().__init__()

    def tick(self, i: int):
        self.frame.clear(xos.color.BLACK)

        # render the number at the center in the frame
        xos.rasterizer.text(str(i), 0.5, 0.5, 24, xos.color.WHITE)

if __name__ == "__main__":
    viewport1 = ViewportTesting()
    viewport2 = ViewportTesting()
    
    for i in range(10000):
        viewport1.tick(i)
        viewport2.tick(i)