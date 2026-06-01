#pragma once
#include <memory>
#include <string>
#include <stdint.h>
#include <functional>
#include "shoop_globals.h"
#include "types.h"
#include <atomic>
#include "LoggingEnabled.h"
#include "RustAudioPort.h"
#include "shoop_shared_ptr.h"
#include "backend_rust/src/command_queue_cxx.rs.h"

enum class ProcessFunctionResult {
    Continue,  // Continue processing next cycle
    Disconnect // Request to be disconnected from the audio processing thread
};

struct AudioMidiDriverSettingsInterface {
    AudioMidiDriverSettingsInterface() {}
    virtual ~AudioMidiDriverSettingsInterface() {}
};

struct ExternalPortDescriptor {
    std::string name;
    shoop_port_direction_t direction;
    shoop_port_data_type_t data_type;
};

class HasAudioProcessingFunction {
public:
    HasAudioProcessingFunction() {}
    virtual ~HasAudioProcessingFunction() {}

    virtual void PROC_process(uint32_t nframes) = 0;
};

class AudioMidiDriver : public ModuleLoggingEnabled<"Backend.AudioMidiDriver">,
                        private shoop_enable_shared_from_this<AudioMidiDriver> {
protected:
    shoop_weak_ptr<AudioMidiDriver> weak_driver_from_this() { return weak_from_this(); }

public:
    virtual void add_processor(shoop_shared_ptr<HasAudioProcessingFunction> p) = 0;
    virtual void remove_processor(shoop_shared_ptr<HasAudioProcessingFunction> p) = 0;
    virtual std::vector<shoop_weak_ptr<HasAudioProcessingFunction>> processors() const = 0;

    virtual void start(AudioMidiDriverSettingsInterface &settings) = 0;

    virtual
    shoop_shared_ptr<RustAudioPortF32> open_audio_port(
        std::string name,
        shoop_port_direction_t direction,
        shoop_shared_ptr<RustAudioPortF32::UsedBufferPool> buffer_pool
    ) = 0;

    virtual
    shoop_shared_ptr<MidiPort> open_midi_port(
        std::string name,
        shoop_port_direction_t direction
    ) = 0;

    virtual shoop_shared_ptr<shoop_types::_DecoupledMidiPort> open_decoupled_midi_port(
        std::string name,
        shoop_port_direction_t direction
    ) = 0;

    virtual void unregister_decoupled_midi_port(shoop_shared_ptr<shoop_types::_DecoupledMidiPort> port) = 0;

    virtual void close() = 0;

    virtual uint32_t get_xruns() const = 0;
    virtual float get_dsp_load() = 0;
    virtual uint32_t get_sample_rate() = 0;
    virtual uint32_t get_buffer_size() = 0;
    virtual void reset_xruns() = 0;
    virtual const char* get_client_name() const = 0;
    virtual void* get_maybe_client_handle() const = 0;
    virtual bool get_active() const = 0;
    virtual uint32_t get_last_processed() const = 0;

    virtual void wait_process() = 0;

    virtual void queue_process_thread_command(std::function<void()> fn) = 0;
    virtual void exec_process_thread_command(std::function<void()> fn) = 0;
    virtual backend_rust::CommandQueue &get_command_queue() = 0;

    virtual std::vector<ExternalPortDescriptor> find_external_ports(
        const char* maybe_name_regex,
        shoop_port_direction_t maybe_direction_filter,
        shoop_port_data_type_t maybe_data_type_filter
    ) = 0;

    AudioMidiDriver() = default;
    virtual ~AudioMidiDriver() {}
};
