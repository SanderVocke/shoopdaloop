# Development mode

When the app is built with `cargo build`, there is a development mode script in the target folder which can be used to run the built binaries against the in-source resources and QML code:

- linux/macos: `shoopdaloop_dev.sh`
- windows: `shoopdaloop_dev.bat`

# Pure egui dummy-engine application

The consolidated egui application uses the real application/backend/engine path with the dummy driver:

```sh
cargo run -p shoopdaloop_egui
```

For browser development:

```sh
cd src/rust/shoopdaloop_egui
trunk serve --open
```

The dummy driver advances engine state but provides no physical audio or MIDI I/O.