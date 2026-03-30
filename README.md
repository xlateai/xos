# xos

A cross-platform application framework with a game-engine core in Rust, designed to be built entirely in Python. Write hardware-accelerated apps once and run them everywhere—iOS, Windows, macOS, Linux, and beyond. From device drivers to viewports, rasterization, and acceleration primitives, everything is exposed through the xos runtime—no glue code, no friction.

A true alternative to React Native: pure Python, zero JavaScript, no HTML or CSS. Build, run, and even edit your apps directly on-device, wherever you are.

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

## Prerequisites

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

## Getting Started
When getting started in xos, you only need to run the cargo commands listed below once. After you've installed xos using cargo, each subsequent change you make to xos can be updated by simply running `xos build` which will automatically rebuild your Rust changes. This is usually unnecessary however, as when attempting to run an application or Python script using xos, it will prompt the user with a Y/n question for if they would like to rebuild Rust or not. Only select Y (default) if you have made changes to Rust. If you haven't, you can decline (selecting 'n').

### Installing xos

Run these once (when first installing xos):

```bash
cargo build --release
cargo install --path .
```

### Explore the `xos` CLI

```bash
xos --help                # List top-level commands available

xos app                   # List available Rust-written xos-driven applications
xos app <app-name>        # Run on your current platform
xos app <app-name> --ios  # Run on connected iOS device

xos python                # Open the Rust python interpreter
xos python <file-path>    # Run a Python file from the xos interpreter (path/to/file.py)
```

### Debugging the CLI

If you run into any weird quirks of the CLI, such as not being able to find a path or seemingly outdated versions of the code being executed when relying on the `xos build` or `xos app {...}` commands for execution, then please `cd` into the xos code directory and run `cargo install --path .`. That should reinstall the CLI and all of the driver code which should fix any issues with `xos build` and or `xos app {...}`. If not, make sure there are no duplicate versions of `xos` running, and be mindful of the path for the executable (`xos path` might help find it).

### The --ios Flag

For launching applications onto your iOS devices:

1. Ensure your iOS device is connected via USB and in **Developer Mode**
2. First-time setup: Open `ios/xos.xcworkspace` in Xcode and configure code signing (Signing & Capabilities tab)
3. Run `xos app <app-name> --ios` - the CLI will build the Rust library (if needed), compile the Swift iOS app, and install/launch on your device

## `xos build`

**Build for your current platform:**
```bash
xos build
```

**Build for iOS:**
```bash
xos build --ios
```
This installs the `aarch64-apple-ios` target if needed, compiles the Rust library as a static library (`.a` file), and outputs to `ios/libs/libxos.a` for linking with Swift code.

## Development Philosophy

xos applications are designed to be:
- **Standalone** - Each app is independent and self-contained
- **Portable** - Write once, run on any supported platform
- **Performant** - Built with Rust for speed and reliability
- **Scientific computing friendly** - Optimized for data visualization, sensor processing, and computational workloads
- **Game engine-like** - Rich rendering capabilities with clean, efficient UI primitives

We're building toward a future where xos applications can be easily modified, extended, and composed together, with a unified home screen and application launcher that makes the entire ecosystem feel cohesive and powerful.
