# Run the application

Native development:

```sh
cargo run -p shoopdaloop
```

The first launch uses the dummy/offline driver. Configure JACK or CPAL+midir under **Settings → Audio**.

Browser development:

```sh
cd src/rust/shoopdaloop
trunk serve --open
```

Use HTTPS or localhost for portable browser audio/MIDI permission behavior.
