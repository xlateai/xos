# xos
xlate's operating system designed to run portably across all types of devices and circumstances.

# Rust
We chose to use rust for this. I've only ever built things like this in C++, and while it's a great language, the build tooling that comes with rust (cargo crates) is just too good. And the modernized windows and "just works" local development is a no-brainer.

# Install CLI
Make sure you have rust installed. It should be super simple.

```
cargo build --release
cargo install --path .
```

After that, you can now run the `xos` CLI, which can be executed in one of a few methods:
1. Standard mode `xos dev` - this will launch the dev showcase app locally on your machine (windows/mac native app windows).
2. Web/wasm mode `xos dev --web` - this will compile the code into WASM and open within your browser.
3. React-native/wasm mode `xos dev --react-native` - this will compile into WASM and launch react-native so you can open the app in the browser OR scan the QR code using expo go to launch the native IOS application!

Since XOS is designed to run anywhere and on any device, it's crucial that all apps developed on xlate are always buildable/runnable through both standard runtimes (compiled for the current target machine) as well as WASM that allows us to launch the exact same app on the browser and on mobile devices like IOS through react native web views.

That means that XOS as a development principle system needs to ensure that conditional compilation requirements for WASM support are always maintained for every app. This means we will likely have to rebuild many pieces of rust's STD libraries. Some examples include `system time`, `rand`, `net`, `fs`, among a few others.

However, it's a small price to pay for full application unity! 

# Update/Develop the CLI
To update and run the CLI, unfortunately Rust doesn't support the ability to automatically sync the CLI command with local changes (the equivalent of python's `pip install -e .` flag for `-e` which adds this feature). So, instead we have to make sure to rebuild the package and THEN run the CLI.

So, after making changes in the repo, you should run:

```
cargo install --path .
xos --help
```

Note: `xos` is our CLI command.

TODO: make it so that xos automatically builds itself (for the standard/native version) whenever we call the CLI so we don't have to run `cargo install --path .` each time we make changes to it (simulating python's `pip install -e .` feature).

# Native Bridge
Don't forget to use the `--tunnel` in `npx expo start --tunnel`, otherwise weird things occur (you'll just see a JSON payload).
