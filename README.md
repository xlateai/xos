# 🐍 xos

One python file, desktop and mobile, built-in viewports, audio drivers, ai/ml/sci compute library, graphics, and text rasterization, and more to come. Everything is tensorized and accelerator ready. Let's see what you can build!

## ℹ️ Details

Xos is a cross-platform application framework with a game-engine core in Rust, designed to be built entirely in Python. Write hardware-accelerated apps once and run them everywhere—iOS, Windows, macOS, Linux, and beyond. From device drivers to viewports, rasterization, and acceleration primitives, everything is exposed through the xos runtime—no glue code, no friction.

- Custom IDE that deploys to your mobile devices so you can code on-the-go
- All apps write directly to tensorized viewports
- Designed to be an alternative to React-Native
- ❌ no JavaScript, no HTML, no CSS ❌

## ⚠️ Experimental Notice

Xos is in an experimental development phase, which means many features may be incomplete. Expect missing tensor operations where expected, quirks on different hardware platforms, and potential breaking changes as we mature towards v1.0.

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

### How to Install

When installing in xos, you only need to run the cargo commands listed below once (or under special cli update circumstances).

Run these once (when first installing xos):

```bash
cargo build --release
cargo install --path .
```

That's it! You should now be able to run the CLI from anywhere and begin launching your python scripts using `xos python code.py`!

### Commands

```bash
xos --help                # List top-level commands available

# Launching xos python applications
xos python                # Open the Rust python interpreter
xos python <file-path>    # Run a Python file from the xos interpreter (path/to/file.py)

# Built-in rust software (convenient and fun built-in applications)
xos app                   # List available Rust-written xos-driven applications
xos app <app-name>        # Run on your current platform
xos app <app-name> --ios  # Run on connected iOS device
```

### The --ios Flag

For launching applications onto your iOS devices:

1. Ensure your iOS device is connected via USB and in **Developer Mode.**
2. First-time setup: Open `ios/xos.xcworkspace` in Xcode and configure code signing (Signing & Capabilities tab).
3. Run `xos app <app-name> --ios` - the CLI will build the Rust library (if needed), compile the Swift iOS app, and install/launch on your device.

Using `--ios` installs the `aarch64-apple-ios` target if needed, compiles the Rust library as a static library (`.a` file), and outputs to `ios/libs/libxos.a` for linking with Swift code.

### Building Rust Changes

- After you've installed xos using cargo, each subsequent change you make to xos can be updated by simply running `xos build` which will automatically rebuild your Rust changes.
- This is usually unnecessary however, as when attempting to run an application or Python script using xos, it will prompt the user with a Y/n question for if they would like to rebuild Rust or not.
- Only select Y (default) if you have made changes to Rust. If you haven't, you can decline (selecting 'n').

### Debugging the CLI

- If you run into any weird quirks of the CLI, such as not being able to find a path or seemingly outdated versions of the code being executed when relying on the `xos build` or `xos app {...}` commands for execution, then please `cd` into the xos code directory and run `cargo install --path .`. That should reinstall the CLI and all of the driver code which should fix any issues with `xos build` and or `xos app {...}`. If not, make sure there are no duplicate versions of `xos` running, and be mindful of the path for the executable (`xos path` might help find it).

## Package Support Limitations

- Many popular packages that Python developers have gotten used to over the years will not be supported in the xos python runtime. The primary reason is that most of the projects like numpy, pytorch, opencv, sounddevice, pillow, etc. are only designed for desktop machines, and generally are versioned incompatibly.
- Often it will take developers hours to set up development environments, relying on package managers like conda, uv, venv, and others to organize versions and requirements.txt files, however at the end of the day this setup takes ages and really only becomes stable when relying on a Dockerfile, which even then can be quite the involved setup process.
- To solve this, `xos` is rebuilding all of the drivers, libraries, and primitives that these packages provide, but into one package and runtime primarily driven by rust and wgpu.
- Consequently, **you cannot launch xos python scripts using standard python environments, you must use the xos command line interface `xos python ./path/to/code.py` in order to develop xos applications**. What's nice about that is it means that all driver versions are perfectly synchronized based on the xos version you use, but also it means that any device that we build and maintain support for (such as ios, macos, windows, linux, android, etc.) your scripts should run natively on-device and optimized to thousands of frames per second.

## Development Philosophy

xos applications are designed to be:

- **Standalone** - Each app is independent and self-contained
- **Portable** - Write once, run on any supported platform
- **Performant** - Built with Rust for speed and reliability
- **Scientific computing friendly** - Optimized for data visualization, sensor processing, and computational workloads
- **Game engine-like** - Rich rendering capabilities with clean, efficient UI primitives

We're building toward a future where xos applications can be easily modified, extended, and composed together, with a unified home screen and application launcher that makes the entire ecosystem feel cohesive and powerful.