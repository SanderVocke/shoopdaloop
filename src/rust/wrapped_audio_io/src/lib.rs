//! Provides traits and functionality to wrap audio I/O libraries which deal directly with
//! devices and streams to present them as JACK-like environments where a client can open
//! and close "ports" on the client.
//! Such "ports" are purely virtual in this case, and "connecting" them just entails mapping
//! them directly to channels on the audio device.

#[cfg(not(feature = "prebuild"))]
mod error;

#[cfg(not(feature = "prebuild"))]
mod host;