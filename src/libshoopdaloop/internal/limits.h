#pragma once 
#include <cstddef>

namespace limits {
constexpr size_t initial_max_loops = 512;
constexpr size_t initial_max_ports = 1024;
constexpr size_t initial_max_fx_chains = 128;
constexpr size_t initial_max_decoupled_midi_ports = 512;
constexpr size_t n_buffers_in_pool = 128;
constexpr size_t audio_buffer_size = 32768;
constexpr size_t command_queue_size = 2048;
constexpr size_t audio_channel_initial_buffers = 128;
constexpr size_t midi_storage_size = 65536;
constexpr size_t default_max_port_mappings = 8;
constexpr size_t default_max_midi_channels = 8;
constexpr size_t default_max_audio_channels = 8;
constexpr size_t decoupled_midi_port_queue_size = 256;
constexpr size_t default_audio_dummy_buffer_size = 16384;
}