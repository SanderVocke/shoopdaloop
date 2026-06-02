#pragma once

#include <cstdint>
#include <functional>
#include <string>
#include <unordered_map>
#include <vector>

#include "BridgeObject.h"
#include "LoggingEnabled.h"
#include "RustCommandQueue.h"
#include "backend_rust/src/audio_midi_driver_cxx.rs.h"
#include "shoop_globals.h"
#include <memory>
#include "types.h"

class AudioMidiDriver;
class HasAudioProcessingFunction;
class MidiPort;

class AudioMidiDriverRuntime : private ModuleLoggingEnabled<"Backend.AudioMidiDriver"> {
public:
    AudioMidiDriverRuntime(void (*maybe_process_callback)() = nullptr);

    void add_processor(std::shared_ptr<HasAudioProcessingFunction> p);
    void remove_processor(std::shared_ptr<HasAudioProcessingFunction> p);
    std::vector<std::weak_ptr<HasAudioProcessingFunction>> processors() const;

    void process(uint32_t nframes);
    void exec_all_commands_for_process_thread();

    void unregister_decoupled_midi_port(std::shared_ptr<shoop_types::_DecoupledMidiPort> port);
    std::shared_ptr<shoop_types::_DecoupledMidiPort> make_decoupled_midi_port(
        std::shared_ptr<MidiPort> port,
        std::weak_ptr<AudioMidiDriver> driver,
        shoop_port_direction_t direction
    );

    uint32_t get_xruns() const;
    void reset_xruns();
    void report_xrun();

    float get_dsp_load() const;
    void set_dsp_load(float load);

    uint32_t get_sample_rate() const;
    void set_sample_rate(uint32_t sample_rate);

    uint32_t get_buffer_size() const;
    void set_buffer_size(uint32_t buffer_size);

    const char* get_client_name() const;
    void set_client_name(const char* name);

    void* get_maybe_client_handle() const;
    void set_maybe_client_handle(void* handle);

    bool get_active() const;
    void set_active(bool active);

    uint32_t get_last_processed() const;
    void set_last_processed(uint32_t nframes);

    void wait_process();

    void queue_process_thread_command(std::function<void()> fn);
    void exec_process_thread_command(std::function<void()> fn);
    backend_rust::CommandQueue &get_command_queue();

private:
    rust::Box<backend_rust::AudioMidiDriverCore> m_rust_core;
    std::vector<std::weak_ptr<HasAudioProcessingFunction>> m_processors;
    std::unordered_map<HasAudioProcessingFunction*, uint64_t> m_processor_handles;
    void (*m_maybe_process_callback)() = nullptr;
    mutable std::string m_client_name_cache;
};
