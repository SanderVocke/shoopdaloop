#pragma once

#include "BridgeObject.h"

class AudioMidiDriver;

using AudioMidiDriverBridgeWeak = BridgeWeak<AudioMidiDriver>;
using AudioMidiDriverBridgeStrong = BridgeStrong<AudioMidiDriver>;
