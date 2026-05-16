#pragma once

// MidiSortingBuffer - Now backed by Rust implementation
// This header provides backward compatibility by including the Rust-backed version

#include "RustMidiSortingBuffer.h"

// Define MidiSortingBuffer as an alias to the Rust-backed implementation
typedef RustMidiSortingBuffer MidiSortingBuffer;