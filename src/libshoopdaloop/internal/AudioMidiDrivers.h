#pragma once
#include "types.h"
#include "AudioMidiDriver.h"
#include <memory>

std::shared_ptr<AudioMidiDriver> create_audio_midi_driver(shoop_audio_driver_type_t type);
bool audio_midi_driver_type_supported(shoop_audio_driver_type_t type);