# 🐍 xos

Python, cross-platform (+ios📱) with built-in viewports, audio drivers, ai/ml/sci compute operations, graphics, text rasterization, and more- tensorized and accelerator ready. Let's see what you can build!

YouTube [@xlateai](https://youtube.com/@xlateai) • X/Twitter [@xlateai](https://x.com/xlateai) • Discord [xlateai](https://discord.gg/WvPaPG7DYh)

## ℹ️ Details

Xos is a cross-platform application framework with a game-engine core in rust with python bindings that feel like numpy/torch, and make graphics pythonic and seamless with scientific computing. Write once and run everywhere—iOS, Windows, macOS, Linux, and beyond.

- All apps write directly to tensorized viewports
- Designed to be an alternative to React-Native
- ❌ no JavaScript, no HTML, no CSS ❌

## 🧠 Available AI Models

Xos makes it easy to use popular model types with standalone easy-to-use native desktop applications. Are you a researcher developing these kinds of models? Feel free to import your own in here and make use of the already robust application code! Application developer/user? Simply launch the applications from our `xos` commandl ine interface OR use `xpy` and call the relevant modules directly. Alternatively, if you're a dev building in rust you can include xos as a dependency in your `Cargo.toml` and reference the relevant rust functions directly! While you're at it, feel free to use our other very useful framework inclusions.

1. Real-time Whisper models is available in 3 major ways:
  1. Usage from the CLI after installation: `xos app transcribe`
  2. Available in the python module `xos.audio.transcribe`.
  3. Available in Rust `transcribe.rs`.
2. TODO: Llama2
3. TODO: OCR

A more helpful documentation on how to use these models is underway, but rest assured that all models will be:

1. Automatically downloaded, converted, and stored so you don't need to find any model weight downloads.
2. Integrated as a standalone CLI-launched desktop application.
3. Integrated as a pythonic module to be used in your own `xpy` scripts.
4. Available in rust so you can build into your own rust projects.

## 🤝 Help Wanted!

Spot any bugs/missing features? [Come join our discord](https://discord.gg/WvPaPG7DYh)! Even if you just want to chat or share what you've built. We would love to have you!

## Progress

- Headless mode for applications without viewports.
- iOS audio drivers.
- iOS haptics drivers.
- Python runtime and scripting.
- Multi-file python.
- Networking.
- Optimized metal and other operations capable high resolution and performance iOS video rendering.
- Locally inferenced chat models.
- Locally inferenced audio transcription models.
- Re-enable WASM/Web support.
- Build for iOS without xcode on the developer's machine.
- Tests + performance checks

## 📁 Code Examples

As of `v0.3.x`, apps are single-file Python scripts run with `**xpy**` (same as `**xos py**` — shorter alias, one binary). Use the xos Python runtime for cross-platform drivers. See `example-scripts`.

- 🚀 `xpy ./example-scripts/ball_lines.py`

```python
# ball_lines.py - example code for xos (`xpy code.py` in terminal to run)
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

That installs `**xos**`, `**xpy**`, and `**xrs**` on your `PATH`. Then run Python scripts with `xpy path/to/code.py` (same as `xos py path/to/code.py`), and Rust apps with `xrs whiteboard` (same as `xos rs whiteboard`).

### 🧰 Common Commands

```bash
xos --help
xos -v
xpy
xpy <file-path>
xrs
xrs <app-name>
xos rs
xos rs <app-name>
xos rs <app-name> --ios
xos compile
xos build
xos path
xos path --exe
```

- `**xpy**` / `**xpy <file>**` is the same as `**xos py**` / `**xos py <file>**` (shorter binary; same CLI).
- `**xrs**` / `**xrs <app-name>**` is the same as `**xos rs**` / `**xos rs <app-name>**` (shorter binary for Rust apps).
- Subcommand aliases: `**xos rust**` / `**xos app**` → `**xos rs**`; `**xos python**` → `**xos py**`.

### 📱 Using `--ios`

Coming soon: RN/Expo-style builds.

1. Connect device by USB and enable **Developer Mode**.
2. Open `ios/xos.xcworkspace` once and configure code signing.
3. Run `xos rs <app-name> --ios`.

`xos` handles Rust target install/build and launches on device.

### 🔁 Recompile + CLI Fixes

- After you change **Rust**, run `**xos compile`** (or `**xos build`** — same subcommand, alias only). There is no automatic compile; the CLI does not rebuild for you.
- If CLI behavior looks stale, run `cargo install --path .`.
- Use `**xos path**` for the repo root (folder with `src/`, where you run `xos compile`), and `**xos path --exe**` for the running executable path.
- `**xos -v**` (or `**xpy -v**`, `**xrs -v**`) prints `xos` / `xpy` / `xrs` and `v<semver>` on the first line, then a second line: full git commit hash, with `**(uncommitted changes)**` in orange when stdout is a terminal and the tree is dirty—or `git tree not available` if there is no usable git checkout.

## 🚧 Package Limitations

- `xos` uses its own runtime APIs, not standard desktop-heavy Python stacks.
- Nearly all third-party packages will be unavailable or incompatible (we'll rebuild it, don't worry).
- Always run scripts via `xpy ./path/to/code.py` or `xos py ./path/to/code.py` (not system Python).

