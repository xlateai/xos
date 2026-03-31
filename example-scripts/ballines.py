import xos

# Ideal vectorized API sketch (1:1 behavior replacement, no per-ball Python loops).
BALL_COLOR = (255, 50, 50, 255)
LINE_COLOR = (10, 80, 80, 255)
BALL_RADIUS = 0.003
LINE_THICKNESS = 0.0008
SPEED_PER_SEC = 0.28


class BallPairsGame(xos.Application):
    def __init__(self):
        super().__init__()
        self.num_balls = 512

    def setup(self):
        n = self.num_balls
        lo, hi = BALL_RADIUS, 1.0 - BALL_RADIUS
        self.pos = xos.random.uniform(shape=(n, 2), low=lo, high=hi)
        self.vel = xos.random.uniform(shape=(n, 2), low=-1.0, high=1.0) * SPEED_PER_SEC
        self.rad = xos.full((n,), BALL_RADIUS)
        idx = xos.arange(0, n, 2, dtype=xos.int32)
        self.pair_idx = xos.stack([idx, idx + 1], axis=1)  # (n/2, 2)
        xos.print(f"+{n} balls in {n // 2} pairs (vectorized)")

    def tick(self):
        self.frame.clear(xos.color.BLACK)
        dt = self.dt

        # Integrate positions
        self.pos = self.pos + self.vel * dt

        # Bounce: flip velocity for any axis that crosses bounds, then clamp
        lo = self.rad[:, None]
        hi = 1.0 - lo
        hit_lo = self.pos < lo
        hit_hi = self.pos > hi
        bounce_mask = hit_lo | hit_hi
        self.vel = xos.where(bounce_mask, -self.vel, self.vel)
        self.pos = xos.clip(self.pos, lo, hi)

        # Convert normalized space to pixels
        w, h = float(self.get_width()), float(self.get_height())
        wh = xos.tensor([w, h]).reshape((1, 2))
        pix = self.pos * wh
        s = max(w, h)

        # Pair lines: gather endpoints from pair indices
        p0 = pix[self.pair_idx[:, 0]]
        p1 = pix[self.pair_idx[:, 1]]
        t = xos.full((self.pair_idx.shape[0],), LINE_THICKNESS * s)
        r = self.rad * s

        xos.rasterizer.lines(self.frame, p0, p1, t, LINE_COLOR)
        xos.rasterizer.circles(self.frame, pix, r, BALL_COLOR)


if __name__ == "__main__":
    BallPairsGame().run()

