#include "BasicLoop.h"
#include "LoggingBackend.h"
#include "types.h"
#include <cstring>
#include <memory>
#include <stdexcept>
#include <utility>
#include <vector>
#include <optional>
#include <iostream>

#ifdef _WIN32
#undef min
#undef max
#endif

// Helper constant for "None" values in FFI
constexpr uint32_t U32_MAX = UINT32_MAX;

BasicLoop::BasicLoop() :
        m_command_queue(100, 1000, 1000),
        m_rust_core(backend_rust::new_basic_loop_core()),
        mp_sync_source(nullptr)
    {
    }

BasicLoop::~BasicLoop() {}

std::optional<uint32_t> BasicLoop::PROC_get_next_poi() const {
    uint32_t when = basic_loop_proc_get_next_poi(*m_rust_core);
    return when == U32_MAX ? std::nullopt : std::optional<uint32_t>(when);
}

std::optional<uint32_t> BasicLoop::PROC_predicted_next_trigger_eta() const {
    uint32_t eta = basic_loop_proc_predicted_next_trigger_eta(*m_rust_core);
    return eta == U32_MAX ? std::nullopt : std::optional<uint32_t>(eta);
}

void BasicLoop::PROC_update_trigger_eta() {
    basic_loop_proc_update_trigger_eta(*m_rust_core);
    
    // Handle sync source - merge with sync source's trigger ETA
    if (mp_sync_source) {
        auto ss_next_trigger = mp_sync_source->PROC_predicted_next_trigger_eta();
        if (ss_next_trigger.has_value()) {
            uint32_t our_trigger = basic_loop_get_next_trigger(*m_rust_core);
            uint32_t merged = our_trigger == U32_MAX ? ss_next_trigger.value() 
                : std::min(our_trigger, ss_next_trigger.value());
            basic_loop_set_next_trigger(*m_rust_core, merged);
        }
    }
}

void BasicLoop::PROC_update_poi() {
    basic_loop_proc_update_poi(*m_rust_core);
}

void BasicLoop::PROC_handle_poi() {
    basic_loop_proc_handle_poi(*m_rust_core);
}

bool BasicLoop::PROC_is_triggering_now() {
    bool triggering = basic_loop_proc_is_triggering_now(*m_rust_core);
    // Also check sync source
    if (mp_sync_source && mp_sync_source->PROC_is_triggering_now()) {
        return true;
    }
    return triggering;
}

void BasicLoop::PROC_process_channels(
    shoop_loop_mode_t mode,
    std::optional<shoop_loop_mode_t> maybe_next_mode,
    std::optional<uint32_t> maybe_next_mode_delay_cycles,
    std::optional<uint32_t> maybe_next_mode_eta,
    uint32_t n_samples,
    uint32_t pos_before,
    uint32_t pos_after,
    uint32_t length_before,
    uint32_t length_after
) {
    // Empty in base class - override in derived classes (AudioMidiLoop)
}

void BasicLoop::PROC_process(uint32_t n_samples) {
    m_command_queue.PROC_handle_command_queue();
    basic_loop_proc_process(*m_rust_core, n_samples);
}

void BasicLoop::set_sync_source(shoop_shared_ptr<LoopInterface> const& src, bool thread_safe) {
    auto fn = [=, this]() {
        mp_sync_source = src;
        // Pass the sync source pointer to Rust (as usize)
        // The Rust side only uses it to check if sync source exists
        size_t ptr = src ? reinterpret_cast<size_t>(src.get()) : 0;
        basic_loop_set_sync_source(*m_rust_core, ptr);
        PROC_update_trigger_eta();
    };
    if(thread_safe) {
        m_command_queue.exec_process_thread_command(fn);
    } else {
        fn();
    }
}

shoop_shared_ptr<LoopInterface> BasicLoop::get_sync_source(bool thread_safe) {
    if(thread_safe) {
        shoop_shared_ptr<LoopInterface> rval;
        m_command_queue.exec_process_thread_command([this, &rval]() { rval = mp_sync_source; });
        return rval;
    }
    return mp_sync_source;
}

void BasicLoop::PROC_update_planned_transition_cache() {
    // This is now handled internally by Rust
    // The cache is updated automatically in plan_transition and PROC_trigger
}

void BasicLoop::PROC_trigger(bool propagate) {
    basic_loop_proc_trigger(*m_rust_core, propagate);
}

void BasicLoop::PROC_handle_sync() {
    if (mp_sync_source && mp_sync_source->PROC_is_triggering_now()) {
        PROC_trigger();
    }
}

void BasicLoop::PROC_handle_transition(shoop_loop_mode_t new_state) {
    log<log_level_debug>("Transition -> {}", (int)new_state);
    basic_loop_proc_handle_transition(*m_rust_core, static_cast<uint32_t>(new_state));
}

uint32_t BasicLoop::get_n_planned_transitions(bool thread_safe) {
    if (thread_safe) {
        uint32_t rval;
        m_command_queue.exec_process_thread_command([this, &rval]() { 
            rval = basic_loop_get_n_planned_transitions(*m_rust_core); 
        });
        return rval;
    }
    return basic_loop_get_n_planned_transitions(*m_rust_core);
}

uint32_t BasicLoop::get_planned_transition_delay(uint32_t idx, bool thread_safe) {
    uint32_t rval;
    auto fn = [this, idx, &rval]() {
        rval = basic_loop_get_planned_transition_delay(*m_rust_core, idx);
    };
    if (thread_safe) {
        m_command_queue.exec_process_thread_command(fn);
    } else {
        fn();
    }
    return rval;
}

shoop_loop_mode_t BasicLoop::get_planned_transition_state(uint32_t idx, bool thread_safe) {
    shoop_loop_mode_t rval;
    auto fn = [this, idx, &rval]() {
        rval = static_cast<shoop_loop_mode_t>(basic_loop_get_planned_transition_state(*m_rust_core, idx));
    };
    if (thread_safe) {
        m_command_queue.exec_process_thread_command(fn);
    } else {
        fn();
    }
    return rval;
}

void BasicLoop::clear_planned_transitions(bool thread_safe) {
    auto fn = [this]() { 
        basic_loop_clear_planned_transitions(*m_rust_core);
    };
    if (thread_safe) {
        m_command_queue.exec_process_thread_command(fn);
    } else {
        fn();
    }
}

void BasicLoop::plan_transition(
    shoop_loop_mode_t mode,
    std::optional<uint32_t> maybe_n_cycles_delay,
    std::optional<uint32_t> maybe_to_sync_cycle,
    bool thread_safe) {
    
    auto fn = [this, mode, maybe_n_cycles_delay, maybe_to_sync_cycle]() {
        bool transitioning_immediately =
            (!mp_sync_source && get_mode() != LoopMode_Playing) ||
            (!maybe_n_cycles_delay.has_value()) ||
            (maybe_to_sync_cycle.has_value());
        
        if (transitioning_immediately) {
            // Un-synced loops transition immediately from non-playing states.
            PROC_handle_transition(mode);
            
            if (maybe_to_sync_cycle.has_value() && mp_sync_source) {
                // Sync up to our sync source immediately
                uint32_t pos =
                    mp_sync_source->get_position() +
                    maybe_to_sync_cycle.value() * mp_sync_source->get_length();
                if (mode == LoopMode_Recording) {
                    set_position(0, false);
                    set_length(pos, false);
                } else {
                    set_position(pos, false);
                }
            }
            clear_planned_transitions(false);
        } else {
            uint32_t n_cycles_delay = maybe_n_cycles_delay.value_or(0);
            basic_loop_plan_transition(*m_rust_core, 
                static_cast<uint32_t>(mode),
                n_cycles_delay,
                U32_MAX); // no sync cycle
        }
        PROC_update_trigger_eta();
    };
    if (thread_safe) { m_command_queue.exec_process_thread_command(fn); }
    else { fn(); }
}

uint32_t BasicLoop::get_position() const {
    return basic_loop_get_position(*m_rust_core);
}

void BasicLoop::set_position(uint32_t position, bool thread_safe) {
    auto fn = [this, position]() {
        basic_loop_set_position(*m_rust_core, position);
    };
    if (thread_safe) { m_command_queue.exec_process_thread_command(fn); }
    else { fn(); }
}

uint32_t BasicLoop::get_length() const {
    return basic_loop_get_length(*m_rust_core);
}

shoop_loop_mode_t BasicLoop::get_mode() const {
    return static_cast<shoop_loop_mode_t>(basic_loop_get_mode(*m_rust_core));
}

void BasicLoop::get_first_planned_transition(shoop_loop_mode_t &maybe_mode_out, uint32_t &delay_out) {
    uint32_t maybe_mode = basic_loop_get_first_planned_transition_mode(*m_rust_core);
    uint32_t maybe_delay = basic_loop_get_first_planned_transition_delay(*m_rust_core);
    
    // In Rust, Unknown mode (0) with delay >= 0 means no planned transition
    // Check: if delay is valid and mode is not Unknown
    if (maybe_delay > 0 || maybe_mode != static_cast<uint32_t>(LoopMode_Unknown)) {
        maybe_mode_out = static_cast<shoop_loop_mode_t>(maybe_mode);
        delay_out = maybe_delay;
    } else {
        maybe_mode_out = LoopMode_Unknown;
        delay_out = 0;
    }
}

void BasicLoop::set_length(uint32_t len, bool thread_safe) {
    log<log_level_debug>("set length: {}", len);

    auto fn = [this, len]() {
        log<log_level_debug_trace>("apply set length: {}", len);
        basic_loop_set_length(*m_rust_core, len);
    };
    if (thread_safe) { m_command_queue.exec_process_thread_command(fn); }
    else { fn(); }
}

void BasicLoop::set_mode(shoop_loop_mode_t mode, bool thread_safe) {
    log<log_level_debug>("set mode: {}", (int)mode);

    auto fn = [this, mode]() {
        log<log_level_debug_trace>("apply set mode: {}", (int)mode);
        basic_loop_set_mode(*m_rust_core, static_cast<uint32_t>(mode));
    };
    if (thread_safe) { m_command_queue.exec_process_thread_command(fn); }
    else { fn(); }
}

std::optional<BasicLoop::PointOfInterest> BasicLoop::dominant_poi(std::optional<PointOfInterest> const& a, std::optional<PointOfInterest> const& b) {
    if(a && !b) { return a; }
    if(b && !a) { return b; }
    if(!a && !b) { return std::nullopt; }
    if(a && b && a.value().when == b.value().when) {
        return PointOfInterest({.when = a.value().when, .type_flags = a.value().type_flags | b.value().type_flags });
    }
    if(a && b && a.value().when < b.value().when) {
        return a;
    }
    return b;
}

std::optional<BasicLoop::PointOfInterest> BasicLoop::dominant_poi(std::vector<std::optional<PointOfInterest>> const& pois) {
    if (pois.size() == 0) { return std::nullopt; }
    if (pois.size() == 1) { return pois[0]; }
    if (pois.size() == 2) { return dominant_poi(pois[0], pois[1]); }
    auto reduced = std::vector<std::optional<PointOfInterest>>(pois.begin()+1, pois.end());
    return dominant_poi(pois[0], dominant_poi(reduced));
}

// Helper methods for AudioMidiLoop to access internal state
std::optional<uint32_t> BasicLoop::internal_get_next_poi_when() const {
    uint32_t when = basic_loop_get_next_poi_when(*m_rust_core);
    return when == U32_MAX ? std::nullopt : std::optional<uint32_t>(when);
}

std::optional<uint32_t> BasicLoop::internal_get_next_trigger_eta() const {
    uint32_t eta = basic_loop_get_next_trigger_eta(*m_rust_core);
    return eta == U32_MAX ? std::nullopt : std::optional<uint32_t>(eta);
}

shoop_loop_mode_t BasicLoop::internal_get_maybe_next_planned_mode() const {
    return static_cast<shoop_loop_mode_t>(basic_loop_get_maybe_next_planned_mode(*m_rust_core));
}

int32_t BasicLoop::internal_get_maybe_next_planned_delay() const {
    return basic_loop_get_maybe_next_planned_delay(*m_rust_core);
}

void BasicLoop::internal_set_next_poi(uint32_t when, unsigned type_flags) {
    basic_loop_set_next_poi(*m_rust_core, when, static_cast<uint32_t>(type_flags));
}