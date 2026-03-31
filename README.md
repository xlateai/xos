# 🐍 xos

One python file, cross-platform (+ios📱), built-in viewports, audio drivers, ai/ml/sci compute library, graphics, text rasterization, and more on the way. Everything is tensorized and accelerator ready. Let's see what you can build!

## ℹ️ Details

Xos is a cross-platform application framework with a game-engine core in rust with python bindings that feel like numpy/torch, and make graphics pythonic and seamless with scientific computing. Write once and run everywhere—iOS, Windows, macOS, Linux, and beyond.

- All apps write directly to tensorized viewports
- Designed to be an alternative to React-Native
- ❌ no JavaScript, no HTML, no CSS ❌

## 🤝 Help Wanted!

Spot any bugs/missing features? [Come join our discord](https://discord.gg/WvPaPG7DYh)! Even if you just want to chat or share what you've built. We would love to have you!

## 📁 Code Example

As of `v0.3.x`, all applications in xos are single-file python scripts launched using the `xos` command line line. It's crucial to use the xos.python runtime since it's what provides all of the convenience drivers across platforms.

- 🚀 `xos python ./code.py`

```python
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
```

## ✅ Getting Started

### ⚠️ Experimental Notice

Xos is in an experimental development phase, which means many features may be incomplete. Expect missing tensor operations where expected, quirks on different hardware platforms, and potential breaking changes as we mature towards v1.0.

### Prerequisites

**Required:**

- ✅ Rust toolchain ([rustup.rs](https://rustup.rs/))

**Optional (iOS):**

- macOS computer
- Xcode
- iOS device with Developer Mode enabled (Settings > Privacy & Security > Developer Mode)
- Physical USB cable connection to iOS device

**Optional (Linux):**

- Advanced Linux Sound Architecture library (for audio support):
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

