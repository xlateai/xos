# xos
Xlate's operating system designed to run portably across all types of devices and circumstances.

# Rust
We chose to use rust for this. I've only ever built things like this in C++, and while it's a great language, the build tooling that comes with rust (cargo crates) is just too good. And the modernized windows and "just works" local development is a no-brainer.

# Install CLI
Make sure you have rust installed. It should be super simple.

```
cargo build --release
cargo install --path .
```