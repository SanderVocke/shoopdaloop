#pragma once
#include <memory>
#include <string>
#include <stdint.h>
#include <functional>
#include "CommandQueue.h"
#include "shoop_globals.h"
#include "types.h"
#include <set>
#include <atomic>
#include "LoggingEnabled.h"
#include "RustAudioPort.h"
#include "shoop_shared_ptr.h"
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

// NOTE (Rust-port migration): this class is progressively being turned into a
// composition-oriented driver façade. Backend-specific behavior (e.g. JACK,
// Dummy) should move behind explicit adapter/ops boundaries and stable type
// metadata, while this class remains the owner of shared core sequencing/state.
class AudioMidiDriver : public ModuleLoggingEnabled<"Backend.AudioMidiDriver">,
                        private shoop_enable_shared_from_this<AudioMidiDriver> {
public:
    // Backend ops compatibility scaffold for migration away from inheritance.
    // Implementations can progressively expose these operations while virtual
    // methods remain the compatibility path.
    struct BackendOps {
        void* ctx = nullptr;
        void (*start)(void* ctx, AudioMidiDriverSettingsInterface &settings) = nullptr;
        shoop_shared_ptr<RustAudioPortF32> (*open_audio_port)(
            void* ctx,
            std::string name,
            shoop_port_direction_t direction,
            shoop_shared_ptr<RustAudioPortF32::UsedBufferPool> buffer_pool) = nullptr;
        shoop_shared_ptr<MidiPort> (*open_midi_port)(
            void* ctx,
            std::string name,
            shoop_port_direction_t direction) = nullptr;
        void (*close)(void* ctx) = nullptr;
    };
private:
    // Rust core for atomic state and processor/decoupled port management
    rust::Box<backend_rust::AudioMidiDriverCore> m_rust_core;
    shoop_shared_ptr<std::vector<shoop_weak_ptr<HasAudioProcessingFunction>>> m_processors;
    std::set<shoop_shared_ptr<shoop_types::_DecoupledMidiPort>> m_decoupled_midi_ports;

    using ProcessHook = void (*)(void* user);
    ProcessHook m_maybe_process_hook = nullptr;
    void* m_maybe_process_hook_user = nullptr;
    BackendOps m_backend_ops{};

protected:
    CommandQueue m_command_queue;

    void set_backend_ops(BackendOps ops) { m_backend_ops = std::move(ops); }
    const BackendOps& backend_ops() const { return m_backend_ops; }
    bool has_backend_ops() const { return m_backend_ops.ctx != nullptr; }

protected:
    // Derived class should call these
    void report_xrun();
    void set_dsp_load(float load);
    void set_sample_rate(uint32_t sample_rate);
    void set_buffer_size(uint32_t buffer_size);
    void set_maybe_client_handle(void* handle);
    void set_client_name(const char* name);
    void set_active(bool active);
    void set_last_processed(uint32_t nframes);

    virtual void maybe_update_sample_rate() {};
    virtual void maybe_update_buffer_size() {};
    virtual void maybe_update_dsp_load() {};

    void PROC_process(uint32_t nframes);

public:
    void add_processor(shoop_shared_ptr<HasAudioProcessingFunction> p);
    void remove_processor(shoop_shared_ptr<HasAudioProcessingFunction> p);
    std::vector<shoop_weak_ptr<HasAudioProcessingFunction>> processors() const;

    // Handle-based APIs for Rust-port migration (ownership remains local).
    void register_processor_handle(uintptr_t handle);
    void unregister_processor_handle(uintptr_t handle);
    std::vector<uintptr_t> processor_handles() const;

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

    shoop_shared_ptr<shoop_types::_DecoupledMidiPort> open_decoupled_midi_port(
        std::string name,
        shoop_port_direction_t direction
    );

    void PROC_process_decoupled_midi_ports(uint32_t nframes);
    void unregister_decoupled_midi_port(shoop_shared_ptr<shoop_types::_DecoupledMidiPort> port);
    void register_decoupled_midi_port_handle(uintptr_t handle);
    void unregister_decoupled_midi_port_handle(uintptr_t handle);
    std::vector<uintptr_t> decoupled_midi_port_handles() const;

    virtual void close() = 0;

    // Stable backend type metadata to avoid RTTI/dynamic_cast based dispatch.
    virtual shoop_audio_driver_type_t driver_type() const = 0;

    uint32_t get_xruns() const;
    float get_dsp_load();
    uint32_t get_sample_rate();
    uint32_t get_buffer_size();
    void reset_xruns();
    const char* get_client_name() const;
    void* get_maybe_client_handle() const;
    bool get_active() const;
    uint32_t get_last_processed() const;

    virtual void wait_process();

    // Command queue forwarding (for external API usage)
    void queue_process_thread_command(std::function<void()> fn) { m_command_queue.queue_process_thread_command(std::move(fn)); }
    void exec_process_thread_command(std::function<void()> fn) { m_command_queue.exec_process_thread_command(std::move(fn)); }
    CommandQueue &get_command_queue() { return m_command_queue; }

    virtual std::vector<ExternalPortDescriptor> find_external_ports(
        const char* maybe_name_regex,
        shoop_port_direction_t maybe_direction_filter,
        shoop_port_data_type_t maybe_data_type_filter
    ) = 0;

    AudioMidiDriver(void (*maybe_process_callback)()=nullptr);
    AudioMidiDriver(ProcessHook maybe_process_hook, void* maybe_process_hook_user);
    virtual ~AudioMidiDriver() {}
};