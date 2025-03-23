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

# Update/Develop the CLI
To update and run the CLI, unfortunately Rust doesn't support the ability to automatically sync the CLI command with local changes (the equivalent of python's `pip install -e .` flag for `-e` which adds this feature). So, instead we have to make sure to rebuild the package and THEN run the CLI.

So, after making changes in the repo, you should run:

```
cargo install --path .
xos --help
```

Note: `xos` is our CLI command.

# Native Bridge
Don't forget to use the `--tunnel` in `npx expo start --tunnel`, otherwise weird things occur (you'll just see a JSON payload).