fn main() {
    println!(
        "{{\"protocol_version\":{},\"command_max_bytes\":{}}}",
        shoop_audio_protocol::PROTOCOL_VERSION,
        shoop_audio_protocol::COMMAND_MAX_BYTES,
    );
}
