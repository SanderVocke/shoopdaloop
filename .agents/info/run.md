# Run the application

Native development:

```sh
cargo run -p shoopdaloop_egui
```

The first launch uses the dummy/offline driver. Configure JACK or CPAL+midir under **Settings → Audio**.

Browser development:

```sh
cd src/rust/shoopdaloop_egui
trunk serve --open
```

Use HTTPS or localhost for portable browser audio/MIDI permission behavior.
