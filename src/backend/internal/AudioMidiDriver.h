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
#include <memory>
#include <vector>
#include "backend_rust/src/command_queue_cxx.rs.h"

class HasAudioProcessingFunction;
class DecoupledMidiPort;

#include "backend_rust/src/audio_midi_driver_cxx.rs.h"

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
                        private std::enable_shared_from_this<AudioMidiDriver> {
protected:
    std::weak_ptr<AudioMidiDriver> weak_driver_from_this() { return weak_from_this(); }

    void set_dsp_load(float load);
    void set_sample_rate(uint32_t sample_rate);
    void set_buffer_size(uint32_t buffer_size);
    void set_client_name(const char* name);
    void set_maybe_client_handle(void* handle);
    void set_active(bool active);
    void set_last_processed(uint32_t nframes);
    void report_xrun();

public:
    void process(uint32_t nframes);
    void exec_all_commands_for_process_thread();
    std::shared_ptr<shoop_types::_DecoupledMidiPort> make_decoupled_midi_port(
        std::shared_ptr<MidiPort> port,
        std::weak_ptr<AudioMidiDriver> driver,
        shoop_port_direction_t direction
    );

    virtual void add_processor(std::shared_ptr<HasAudioProcessingFunction> p);
    virtual void remove_processor(std::shared_ptr<HasAudioProcessingFunction> p);
    virtual std::vector<std::weak_ptr<HasAudioProcessingFunction>> processors() const;

    virtual void start(AudioMidiDriverSettingsInterface &settings) = 0;

    virtual
    std::shared_ptr<RustAudioPortF32> open_audio_port(
        std::string name,
        shoop_port_direction_t direction,
        std::shared_ptr<RustAudioPortF32::UsedBufferPool> buffer_pool
    ) = 0;

    virtual
    std::shared_ptr<MidiPort> open_midi_port(
        std::string name,
        shoop_port_direction_t direction
    ) = 0;

    virtual std::shared_ptr<shoop_types::_DecoupledMidiPort> open_decoupled_midi_port(
        std::string name,
        shoop_port_direction_t direction
    ) = 0;

    virtual void unregister_decoupled_midi_port(std::shared_ptr<shoop_types::_DecoupledMidiPort> port);

    virtual void close() = 0;

    virtual uint32_t get_xruns() const;
    virtual float get_dsp_load();
    virtual uint32_t get_sample_rate();
    virtual uint32_t get_buffer_size();
    virtual void reset_xruns();
    virtual const char* get_client_name() const;
    virtual void* get_maybe_client_handle() const;
    virtual bool get_active() const;
    virtual uint32_t get_last_processed() const;

    virtual void wait_process();

    virtual void queue_process_thread_command(std::function<void()> fn);
    virtual void exec_process_thread_command(std::function<void()> fn);
    virtual backend_rust::CommandQueue &get_command_queue();

    virtual std::vector<ExternalPortDescriptor> find_external_ports(
        const char* maybe_name_regex,
        shoop_port_direction_t maybe_direction_filter,
        shoop_port_data_type_t maybe_data_type_filter
    ) = 0;

    AudioMidiDriver(void (*maybe_process_callback)() = nullptr);
    virtual ~AudioMidiDriver() {}

private:
    rust::Box<backend_rust::AudioMidiDriverCore> m_rust_core;
    void (*m_maybe_process_callback)() = nullptr;
    mutable std::string m_client_name_cache;
};
