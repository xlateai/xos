"""
Depthwise convolution demo - 256x256 separate tensor, convolve each step, render centered.
"""
import xos


class DepthwiseConvolution(xos.Application):
    def __init__(self):
        super().__init__()
        self.kernel = None
        self.conv_image = None
    
    def setup(self):
        """Initialize once: random 3x3 depthwise kernel and 256x256 random image tensor"""
        xos.print("Depthwise Convolution Demo initialized")
        self.kernel = xos.random.uniform(-1.0, 1.0, shape=(3, 3), dtype=xos.float32)
        self.conv_image = xos.random.uniform(0.0, 255.0, shape=(256, 256, 3))
        xos.print("256x256 conv tensor, single kernel")
    
    def tick(self):
        self.conv_image = xos.ops.convolve_depthwise_image(self.conv_image, self.kernel)
        xos.rasterizer.draw_image_centered(self.frame.array, self.conv_image)


if __name__ == "__main__":
    xos.print("Depthwise Convolution Demo")
    app = DepthwiseConvolution()
    app.run()
