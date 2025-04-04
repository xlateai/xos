# xos
xlate's operating system designed to run portably across all types of devices and circumstances.

# Rust
Yes rust. We're so modern look at us go. But seriously, what an incredibly well put-together ecosystem.

# Using the CLI
Make sure you have rust installed. It should be super simple.

```
cargo build --release
cargo install --path .
```

Advanced Linux Sound Architecture library is needed if you do not have it
```
sudo apt-get install libasound2-dev
sudo dnf install alsa-lib-devel
sudo pacman -S alsa-lib
```

To run the react native, expo & its dependencies are required
```
cd src/native-bridge/
npm i -g
```

After that, you can now run the `xos` CLI, which can be executed in one of a few methods:
1. `xos dev` (standard) - this will launch the dev showcase app locally on your machine (windows/mac native app windows).
2. `xos dev --web` (web/wasm) - this will compile the code into WASM and open within your browser.
3. `xos dev --react-native` (react-native/wasm) - this will compile into WASM and launch react-native so you can open the app in the browser OR scan the QR code using expo go to launch the native IOS application!

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

# Python (EXPERIMENTAL)
First, you'll need to have python installed on your machine, also preferably inside of a venv (I prefer conda).

```
pip install maturin
```

Then, from the root of the repo, you can run:

```
maturin develop --release
```

Note: `--release` is really important for performance, even if you're just developing.