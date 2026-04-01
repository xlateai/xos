# 🐍 xos

Python, cross-platform (+ios📱) with built-in viewports, audio drivers, ai/ml/sci compute operations, graphics, text rasterization, and more- tensorized and accelerator ready. Let's see what you can build!


YouTube [@xlateai](https://youtube.com/@xlateai) • X/Twitter [@xlateai](https://x.com/xlateai) • Discord [xlateai](https://discord.gg/WvPaPG7DYh)

## ℹ️ Details

Xos is a cross-platform application framework with a game-engine core in rust with python bindings that feel like numpy/torch, and make graphics pythonic and seamless with scientific computing. Write once and run everywhere—iOS, Windows, macOS, Linux, and beyond.

- All apps write directly to tensorized viewports
- Designed to be an alternative to React-Native
- ❌ no JavaScript, no HTML, no CSS ❌

## 🤝 Help Wanted!

Spot any bugs/missing features? [Come join our discord](https://discord.gg/WvPaPG7DYh)! Even if you just want to chat or share what you've built. We would love to have you!

## Progress
- [x] Headless mode for applications without viewports.
- [x] iOS audio drivers.
- [ ] iOS haptics drivers.
- [x] Python runtime and scripting.
- [ ] Networking.
- [ ] Optimized metal and other operations capable high resolution and performance iOS video rendering.
- [ ] Locally inferenced chat models.
- [ ] Locally inferenced audio transcription models.
- [ ] Re-enable WASM/Web support.
- [ ] Build for iOS without xcode on the developer's machine.
- [ ] Tests + performance checks

## 📁 Code Examples

As of `v0.3.x`, all applications in xos are single-file python scripts launched using the `xos` command line line. It's crucial to use the xos.python runtime since it's what provides all of the convenience drivers across platforms. Check the `example-scripts` folder for more examples.

- 🚀 `xos python ./example-scripts/ball_lines.py`

```python
# ball_lines.py - example code for xos (`xos python code.py` in terminal to run)
import xos

# Ideal vectorized API sketch (1:1 behavior replacement, no per-ball Python loops).
BALL_COLOR = (255, 50, 50, 255)
LINE_COLOR = (10, 80, 80, 255)
BALL_RADIUS = 0.003
LINE_THICKNESS = 0.0008
SPEED_PER_SEC = 0.28


class BallLines(xos.Application):
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

        # Integrate positions
        self.pos = self.pos + self.vel * self.dt

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
    BallLines().run()
```

## ✅ Getting Started

### 🚧 Experimental

Xos is in an experimental development phase, which means many features may be incomplete. Expect missing tensor operations where expected, quirks on different hardware platforms, and potential breaking changes as we mature towards v1.0.

### Prerequisites

- ✅ Rust toolchain ([rustup.rs](https://rustup.rs/))

iOS extras

- macOS computer
- Xcode
- iOS device with Developer Mode enabled (Settings > Privacy & Security > Developer Mode)
- Physical USB cable connection to iOS device

Linux extras

- Advanced Linux Sound Architecture library (audio):
  ```bash
  sudo apt-get update && sudo apt-get install -y libasound2-dev
  ```

### ⚡ How to Install

Run once:

```bash
cargo build --release
cargo install --path .
```

Then run apps anywhere with `xos python code.py`.

### 🧰 Common Commands

```bash
xos --help
xos -v
xos python
xos python <file-path>
xos app
xos app <app-name>
xos app <app-name> --ios
xos compile
xos build
xos path
xos path --exe
```

### 📱 Using `--ios`

Coming soon: RN/Expo-style builds.

1. Connect device by USB and enable **Developer Mode**.
2. Open `ios/xos.xcworkspace` once and configure code signing.
3. Run `xos app <app-name> --ios`.

`xos` handles Rust target install/build and launches on device.

### 🔁 Recompile + CLI Fixes

- After Rust changes, run **`xos compile`** (or **`xos build`** — same command, alias only).
- If CLI behavior looks stale, run `cargo install --path .`.
- Use **`xos path`** for the repo root (folder with `src/`, where you run `xos compile`), and **`xos path --exe`** for the running executable path.
- **`xos -v`** prints the semver only (e.g. `0.3.6`).

## 🚧 Package Limitations

- `xos` uses its own runtime APIs, not standard desktop-heavy Python stacks.
- Nearly all third-party packages will be unavailable or incompatible (we'll rebuild it, don't worry).
- Always run scripts via `xos python ./path/to/code.py` (not system Python).

