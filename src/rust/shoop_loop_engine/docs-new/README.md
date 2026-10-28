# shoop_loop_engine

Draft opening of the replacement book. The existing documentation remains unchanged.

## Part I — Record and play audio

- [1. The loop engine and the host](part-1-record-and-play-audio/01-the-loop-engine-and-the-host.md)
- [2. Preparing an audio loop](part-1-record-and-play-audio/02-preparing-an-audio-loop.md)
- [3. Recording, playing, and stopping](part-1-record-and-play-audio/03-recording-playing-and-stopping.md)

## Run the example

The [first loop example](examples/first_loop.rs) records three stereo frames, plays them immediately, wraps, and stops. It checks the samples, request outcomes, and resource recovery without an audio device.

Run it from the repository root:

```sh
cargo run --locked --manifest-path src/rust/shoop_loop_engine/docs-new/examples/Cargo.toml --bin first_loop
```

Add `--release` for the optimized build. The example has a separate Cargo manifest so this draft does not change the library's existing examples or workspace configuration.
