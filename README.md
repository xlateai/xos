# xos

You can watch a 30 minute primer on [what XOS is and it's future on YouTube here](https://www.youtube.com/watch?v=01APSubyLoQ).

xos is a high-performance, cross-platform application framework for building interactive, compute-intensive software with a single, coherent architecture. It is designed to unify concepts from UI frameworks, game engines, tensor operator libraries, and experimental operating-system–like runtimes into a lightweight build system.

xos enables applications to target multiple platforms with minimal code divergence, providing explicit and ergonomic mechanisms for device-specific specialization, backend selection, and responsive layout across screen sizes.

Applications built with xos are standalone programs that run on any supported backend. Implemented in Rust, xos leverages a modern build system, strong safety guarantees, and predictable performance, while maintaining broad platform compatibility and low-level control where required. This makes xos well-suited for rapid prototyping, research tooling, and experimental systems that span UI, graphics, and numerical computation.

## Experimental Status
The xos API is currently experimental and subject to change. In particular, the application engine interface is evolving to support headless execution, sub-frame embedding, and cross-application interoperability.

Planned extensions include Python-based scripting for application definition and computational workflows, with automatic delegation to available hardware accelerators—enabling lightweight, PyTorch-style numerical experimentation within the xos runtime.

**Progress:**
- [ ] Headless mode for applications without viewports.
- [ ] iOS audio drivers.
- [ ] iOS haptics drivers.
- [ ] Python runtime and scripting.
- [ ] Networking.
- [ ] Optimized metal and other operations capable high resolution and performance iOS video rendering.
- [ ] Locally evaluating chat language models.

## Platform Support

### ✅ Fully Supported

- **macOS** - Native desktop applications
- **iOS** - Native mobile applications (requires macOS + Xcode)
- **Windows** - Native desktop applications
- **Linux** - Native desktop applications (with optional ALSA for audio)

### ⏸️ Paused (Planned Return)

- **Web/WASM** - Browser-based execution (planned for future release)

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

### Installing xos

From the root of the repository:

```bash
cargo build --release
cargo install --path .
```

Verify the CLI is working:

```bash
xos --help
```

## Running Applications

```bash
xos app <app-name>        # Run on your current platform
xos app <app-name> --ios  # Run on connected iOS device
```

List all available applications:
```bash
xos app
```

For launching applications onto your iOS devices:
1. Ensure your device is connected via USB and in Developer Mode
2. First-time setup: Open `ios/xos.xcworkspace` in Xcode and configure code signing (Signing & Capabilities tab)
3. Run `xos app <app-name> --ios` - the CLI will build the Rust library (if needed), compile the Swift iOS app, and install/launch on your device

## Developing Applications

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
