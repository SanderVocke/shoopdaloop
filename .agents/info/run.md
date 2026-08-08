# Development mode

When the app is built with `cargo build`, there is a development mode script in the target folder which can be used to run the built binaries against the in-source resources and QML code:

- linux/macos: `shoopdaloop_dev.sh`
- windows: `shoopdaloop_dev.bat`

# Pure egui application

The consolidated egui application uses the real application/backend/engine path. Native builds start the persisted JACK, CPAL+midir, or dummy/offline configuration; first run defaults to dummy. Hosted browser builds use Web Audio:

```sh
cargo run -p shoopdaloop_egui
```

For browser development:

```sh
cd src/rust/shoopdaloop_egui
trunk serve --open
```

Open the native **Settings → Audio** tab to discover drivers/devices and perform a confirmation-gated runtime switch. The dummy driver advances engine state but provides no physical audio or MIDI I/O.