#pragma once
#include <optional>
#include <memory>
#include <stdio.h>
#include "MidiPort.h"
#include <string>
#include <map>
#include "AudioPort.h"
#include "InternalAudioPort.h"
#include <functional>
#include "shoop_shared_ptr.h"

template<typename TimeType, typename SizeType>
class ProcessingChainInterface {
public:
    using _InternalAudioPort = InternalAudioPort<float>;
    using SharedInternalAudioPort = shoop_shared_ptr<InternalAudioPort<float>>;
    using SharedMidiPort = shoop_shared_ptr<MidiPort>;

    ProcessingChainInterface() {}
    virtual ~ProcessingChainInterface() {}

    virtual
    std::vector<SharedInternalAudioPort> const& input_audio_ports() const = 0;

    virtual
    std::vector<SharedInternalAudioPort> const& output_audio_ports() const = 0;

    virtual
    std::vector<SharedMidiPort> const& input_midi_ports() const = 0;

    // TODO: support MIDI output.

    virtual bool is_freewheeling() const = 0;
    virtual void set_freewheeling(bool enabled) = 0;

    virtual void process(uint32_t frames) = 0;

    // When a processing chain is instantiated, it may not immediately be
    // ready to process.
    // A non-ready processing chain already has valid ports but has undefined
    // behaviour as to what will happen on those ports.
    virtual bool is_ready() const = 0;

    virtual void stop() = 0;

    // A processing chain may be deactivated, in which case it will not produce
    // any output. Typical use-case is for saving CPU.
    virtual bool is_active() const = 0;
    virtual void set_active(bool active) = 0;
};