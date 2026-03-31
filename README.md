# 🐍 xos

One python file, cross-platform (+ios📱), built-in viewports, audio drivers, ai/ml/sci compute library, graphics, text rasterization, and more on the way. Everything is tensorized and accelerator ready. Let's see what you can build!

## ℹ️ Details

Xos is a cross-platform application framework with a game-engine core in rust with python bindings that feel like numpy/torch, and make graphics pythonic and seamless with scientific computing. Write once and run everywhere—iOS, Windows, macOS, Linux, and beyond.

- All apps write directly to tensorized viewports
- Designed to be an alternative to React-Native
- ❌ no JavaScript, no HTML, no CSS ❌

## 🤝 Help Wanted!

Spot any bugs/missing features? [Come join our discord](https://discord.gg/WvPaPG7DYh)! Even if you just want to chat or share what you've built. We would love to have you!

## 📁 Code Examples

As of `v0.3.x`, all applications in xos are single-file python scripts launched using the `xos` command line line. It's crucial to use the xos.python runtime since it's what provides all of the convenience drivers across platforms. Check the `example-scripts` folder for more examples.

- 🚀 `xos python ./example-scripts/ball.py`

```python
# ball.py - example code for xos (`xos python code.py` in terminal to run)
import xos

BALL_COLOR = (255, 50, 50, 255)
BALL_RADIUS = 0.03

class BallDemo(xos.Application):
    headless: bool = False  # optional flag to disable viewport display (helpful for ml/rl)

    def setup(self):
        self.x, self.y = 0.5, 0.5
        self.vx, self.vy = 0.006, 0.004

    def tick(self):
        self.frame.clear(xos.color.BLACK)
        self.x += self.vx
        self.y += self.vy
        if self.x - BALL_RADIUS < 0 or self.x + BALL_RADIUS > 1:
            self.vx *= -1
            self.x = max(BALL_RADIUS, min(1 - BALL_RADIUS, self.x))
        if self.y - BALL_RADIUS < 0 or self.y + BALL_RADIUS > 1:
            self.vy *= -1
            self.y = max(BALL_RADIUS, min(1 - BALL_RADIUS, self.y))
        w, h = self.get_width(), self.get_height()
        r = BALL_RADIUS * max(w, h)
        xos.rasterizer.circles(self.frame, [(self.x * w, self.y * h)], [r], BALL_COLOR)

if __name__ == "__main__":
    BallDemo().run()
```

## ✅ Getting Started

### 🚧 Experimental

Xos is in an experimental development phase, which means many features may be incomplete. Expect missing tensor operations where expected, quirks on different hardware platforms, and potential breaking changes as we mature towards v1.0.

### Prerequisites

- ✅ Rust toolchain ([rustup.rs](https://rustup.rs/))
<details><summary>iOS extras</summary>

- macOS computer
- Xcode
- iOS device with Developer Mode enabled (Settings > Privacy & Security > Developer Mode)
- Physical USB cable connection to iOS device

</details>
<details><summary>Linux extras</summary>

- Advanced Linux Sound Architecture library (audio):
  ```bash
  sudo apt-get update && sudo apt-get install -y libasound2-dev
  ```

</details>

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
xos python
xos python <file-path>
xos app
xos app <app-name>
xos app <app-name> --ios
```

### 📱 Using `--ios`

1. Connect device by USB and enable **Developer Mode**.
2. Open `ios/xos.xcworkspace` once and configure code signing.
3. Run `xos app <app-name> --ios`.

`xos` handles Rust target install/build and launches on device.

### 🔁 Rebuild + CLI Fixes

- Use `xos build` after Rust changes.
- On launch prompts, pick `Y` only if Rust changed.
- If CLI behavior looks stale, run `cargo install --path .`.
- Use `xos path` to verify which executable is running.

## 🚧 Package Limitations

- `xos` uses its own runtime APIs, not standard desktop-heavy Python stacks.
- Nearly all third-party packages will be unavailable or incompatible (we'll rebuild it, don't worry).
- Always run scripts via `xos python ./path/to/code.py` (not system Python).

