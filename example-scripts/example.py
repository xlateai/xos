# code.py - example viewport application code from xos (`xos python code.py` in terminal to run)
import xos

class XOSExample(xos.Application):
    headless: bool = False  # optional flag to disable viewport display
    def tick(self):
        self.frame.clear(xos.color.BLACK)
        text = "welcome to xos"
        w, h = self.get_width(), self.get_height()
        size = 28.0
        x = float((w - len(text) * size * 0.5) / 2)
        y = float((h - size) / 2)
        xos.rasterizer.text(text, x, y, size, (255, 255, 255), float(w))

if __name__ == "__main__":
    XOSExample().run()