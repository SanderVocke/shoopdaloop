#pragma once
#include "types.h"
#include "AudioMidiDriverInterface.h"
#include <memory>

AudioMidiDriverInterface &create_audio_midi_driver(shoop_audio_driver_type_t type);

bool audio_midi_driver_type_supported(shoop_audio_driver_type_t type);