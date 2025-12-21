# xos

The fastest and easiest to use cross-platform application framework designed to feel like a UI framework, game engine, tensor operator library, operating system, and mobile scientific computation experimentation playground.

Apps in xos are standalone programs that run on any supported backend. Built with Rust for it's build system, performance, wide backend target support, and reliability; xos provides an easy foundation to build upon for your prototype applications and experiments.

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

## Running Applications:
```bash
xos app <app-name>        # Run on your current platform
xos app <app-name> --ios  # Run on connected iOS device
```

List all available applications:
```bash
xos app
```

**For iOS deployment:**
1. Ensure your device is connected via USB and in Developer Mode
2. First-time setup: Open `ios/xos.xcworkspace` in Xcode and configure code signing (Signing & Capabilities tab)
3. Run `xos app <app-name> --ios` - the CLI will build the Rust library (if needed), compile the Swift iOS app, and install/launch on your device

## Platform Support

### ✅ Fully Supported

- **macOS** - Native desktop applications
- **Linux** - Native desktop applications (with optional ALSA for audio)
- **iOS** - Native mobile applications (requires macOS + Xcode)

### ⏸️ Paused (Planned Return)

- **Web/WASM** - Browser-based execution (planned for future release)

## Development Philosophy

xos applications are designed to be:
- **Standalone** - Each app is independent and self-contained
- **Portable** - Write once, run on any supported platform
- **Performant** - Built with Rust for speed and reliability
- **Scientific computing friendly** - Optimized for data visualization, sensor processing, and computational workloads
- **Game engine-like** - Rich rendering capabilities with clean, efficient UI primitives

We're building toward a future where xos applications can be easily modified, extended, and composed together, with a unified home screen and application launcher that makes the entire ecosystem feel cohesive and powerful.
