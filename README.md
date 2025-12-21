# xos

xlate's operating system—a cross-platform framework designed to feel like a game engine and tensor library wrapped up into an easy to use and understand programming interface.

Every application in xos is essentially like a standalone application in a standard operating system, but you can launch them on any supported backend. Built with Rust for performance, reliability, and universal compatibility, xos aims to become as easily cross-compatible as possible—a perfect build system with universal applicability that feels more like a game engine with conveniences always within reach.

**Key goals:**
- **<1 minute build times** across the board
- **No HTML/CSS dependencies**—rasterization/game engine approach with clean UI optimized for scientific computing
- **Universal application compatibility**—write once, run anywhere
- **Future "home screen"** where applications, launchers, and drivers are easily modifiable and compatible

## Prerequisites

**Required:**
- ✅ Rust toolchain ([rustup.rs](https://rustup.rs/))
- ✅ macOS (for iOS development)
- ✅ Xcode (for iOS development)
- ✅ iOS device with Developer Mode enabled (for iOS deployment)

**Optional (Linux):**
- Advanced Linux Sound Architecture library (for audio support):
  ```bash
  sudo apt-get update && sudo apt-get install -y libasound2-dev
  ```

## Quick Start

### Installing the xos CLI

From the root of the repository:

```bash
cargo build --release
cargo install --path .
```

After installation, verify the CLI is working:

```bash
xos --help
```

**Note:** When developing the CLI itself, rebuild after making changes:

```bash
cargo install --path .
```

## Building xos

### Standard Build

Build xos for your current platform:

```bash
xos build
```

This compiles the Rust core library for your native platform (macOS, Linux, or Windows).

### iOS Build

Build the Rust library for iOS devices:

```bash
xos build --ios
```

This script:
- Installs the `aarch64-apple-ios` target if needed
- Compiles the Rust library as a static library (`.a` file)
- Outputs to `ios/libs/libxos.a` for linking with Swift code

**iOS Requirements:**
- macOS computer
- Physical USB cable connection to iOS device
- Device must be in Developer Mode (Settings > Privacy & Security > Developer Mode)
- Xcode with proper code signing configured

## Running Applications

xos applications are standalone programs that can run on any supported backend. Each application is like a native OS application but with universal portability.

### Running on Native Platform

Run any xos application on your current platform:

```bash
xos app <app-name>
```

**Available applications:**
- `Ball` - Ball physics demo
- `Tracers` - Particle tracer visualization
- `Camera` - Camera capture app
- `Whiteboard` - Drawing whiteboard
- `Text` - Text editor
- `Waveform` - Audio waveform visualization
- `Wireframe` - 3D wireframe demo
- `Triangles` - Triangle rendering demo
- `Audiovis` - Audio visualization
- `AudioEdit` - Audio editor
- `IosSensors` - iOS sensor data visualization
- And more...

### Running on iOS

Launch an application on a connected iOS device:

```bash
xos app <app-name> --ios
```

**iOS Deployment Process:**
1. Ensure your device is connected via USB and in Developer Mode
2. Run `xos build --ios` to compile the Rust library
3. Run `xos app <app-name> --ios` to build and deploy

The CLI will:
- Build the Rust library for iOS (if needed)
- Compile the Swift iOS app
- Install and launch on your connected device

**First-time iOS setup:**
- Open `ios/xos.xcworkspace` in Xcode
- Configure code signing (Signing & Capabilities tab)
- Select your development team
- The CLI handles the rest automatically

## Platform Support

### ✅ Fully Supported

- **macOS** - Native desktop applications
- **Linux** - Native desktop applications (with optional ALSA for audio)
- **iOS** - Native mobile applications (requires macOS + Xcode)

### ⏸️ Paused (Planned Return)

- **Web/WASM** - Browser-based execution (planned for future release)

## Project Structure

```
xos/
├── src/
│   ├── apps/          # xos applications
│   ├── engine/        # Core engine and application framework
│   ├── sensors/       # Sensor abstractions
│   ├── audio/         # Audio processing
│   ├── video/         # Video processing
│   └── main.rs        # CLI entry point
├── ios/               # iOS native app (Swift)
│   ├── libs/          # Compiled Rust libraries
│   └── xos.xcworkspace
├── build-ios.sh       # iOS Rust library build script
└── Cargo.toml         # Rust project configuration
```

## Common Tasks

**Rebuild iOS Rust library:**
```bash
xos build --ios
# Or manually:
./build-ios.sh
```

**Run an app on iOS:**
```bash
xos app IosSensors --ios
```

**List available apps:**
```bash
xos app --help
```

**Rebuild CLI after changes:**
```bash
cargo install --path .
```

## Development Philosophy

xos applications are designed to be:
- **Standalone** - Each app is independent and self-contained
- **Portable** - Write once, run on any supported platform
- **Performant** - Built with Rust for speed and reliability
- **Scientific computing friendly** - Optimized for data visualization, sensor processing, and computational workloads
- **Game engine-like** - Rich rendering capabilities with clean, efficient UI primitives

We're building toward a future where xos applications can be easily modified, extended, and composed together, with a unified home screen and application launcher that makes the entire ecosystem feel cohesive and powerful.
