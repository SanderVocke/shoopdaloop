#include "BasicLoop.h"

BasicLoop::BasicLoop() :
        WithCommandQueue<100, 1000, 1000>(),
        mp_next_poi(std::nullopt),
        mp_next_trigger(std::nullopt),
        ma_mode(Stopped),
        mp_sync_source(nullptr),
        ma_triggering_now(false),
        ma_length(0),
        ma_position(0),
        ma_maybe_next_planned_mode(LOOP_MODE_INVALID),
        ma_maybe_next_planned_delay(-1),
        ma_already_triggered(false)
    {}

BasicLoop::~BasicLoop() {}

std::optional<size_t> BasicLoop::PROC_get_next_poi() const {
    return mp_next_poi.has_value() ? mp_next_poi.value().when : (std::optional<size_t>)std::nullopt;
}

std::optional<size_t> BasicLoop::PROC_predicted_next_trigger_eta() const {
    return mp_next_trigger;
}

inline bool is_playing_mode(loop_mode_t mode) {
    return mode == Playing ||
            mode == Replacing ||
            mode == PlayingDryThroughWet ||
            mode == RecordingDryIntoWet;
}

void BasicLoop::PROC_update_trigger_eta() {
    if (is_playing_mode(ma_mode) && ma_position < ma_length) {
        mp_next_trigger = ma_length - ma_position;
    } else {
        mp_next_trigger = std::nullopt;
    }

    if (mp_sync_source) {
        auto ss_next_trigger = mp_sync_source->PROC_predicted_next_trigger_eta();
        if (ss_next_trigger.has_value()) {
            mp_next_trigger = mp_next_trigger.has_value() ? std::min (mp_next_trigger.value(), ss_next_trigger.value()) : ss_next_trigger;
        }
    }
}

void BasicLoop::PROC_update_poi() {
    if(is_playing_mode(ma_mode) && 
        ma_length == 0) {
        PROC_handle_transition(Stopped);
    }

    if(mp_next_poi) {
        // Loop end an dhannel POIs will be re-calculated.
        mp_next_poi->type_flags &= ~(LoopEnd);
        mp_next_poi->type_flags &= ~(ChannelPOI);
        if (mp_next_poi->type_flags == 0) {
            mp_next_poi = std::nullopt;
        }
    }

    std::optional<PointOfInterest> loop_end_poi;
    if (is_playing_mode(ma_mode) && ma_position < ma_length) {
        std::optional<PointOfInterest> loop_end_poi = PointOfInterest {
            .when = ma_length - ma_position,
            .type_flags = LoopEnd
        };
        mp_next_poi = dominant_poi(mp_next_poi, loop_end_poi);
    }
}

void BasicLoop::PROC_handle_poi() {
    // Only handle POIs that are reached right now.
    if (!mp_next_poi || mp_next_poi.value().when != 0) {
        return;
    }
    bool changed = false;

    mp_next_poi->type_flags &= ~(ChannelPOI);

    if (mp_next_poi->type_flags & Trigger) {
        PROC_trigger();
        mp_next_poi->type_flags &= ~(Trigger);
        changed = true;
    }
    if (mp_next_poi->type_flags & LoopEnd) {
        mp_next_poi->type_flags &= ~(LoopEnd);
        if (!mp_sync_source || !is_playing_mode(mp_sync_source->get_mode())) {
            // Trigger ourselves if sync source not active.
            PROC_trigger();
        }
        changed = true;
    }

    if (mp_next_poi->type_flags == 0) {
        mp_next_poi = std::nullopt;
        changed = true;
    }

    if (changed) {
        PROC_update_poi();
        PROC_update_trigger_eta();
    }
}

bool BasicLoop::PROC_is_triggering_now() {
    if (mp_next_poi.has_value() && mp_next_poi.value().when == 0) {
        PROC_handle_poi();
    }
    if (mp_sync_source && mp_sync_source->PROC_is_triggering_now()) { return true; }
    if (ma_triggering_now) { return true; }
    return false;
}

void BasicLoop::PROC_process_channels(
    loop_mode_t mode,
    std::optional<loop_mode_t> maybe_next_mode,
    std::optional<size_t> maybe_next_mode_delay_cycles,
    std::optional<size_t> maybe_next_mode_eta,
    size_t n_samples,
    size_t pos_before,
    size_t pos_after,
    size_t length_before,
    size_t length_after
) {}

void BasicLoop::PROC_process(size_t n_samples) {
    if (n_samples == 0) {
        throw std::runtime_error("Processing by 0");
    }
    if (mp_next_poi && n_samples > mp_next_poi.value().when) {
        throw std::runtime_error("Attempted to process loop beyond its next POI.");
    }
    PROC_handle_command_queue();

    ma_triggering_now = false;
    ma_already_triggered = false;

    size_t pos_before = ma_position;
    size_t pos_after = ma_position;
    size_t length_before = ma_length;
    size_t length_after = ma_length;

    loop_mode_t process_channel_mode = ma_mode;

    switch(ma_mode) {
        case Recording:
            length_after += n_samples;
            break;
        case Replacing:
            pos_after += n_samples;
            length_after = std::max(length_after, pos_after);
            break;
        case Playing:
        case PlayingDryThroughWet:
        case RecordingDryIntoWet:
            pos_after = std::min(pos_after + n_samples, length_after);
            if (pos_after == pos_before) {
                process_channel_mode = Stopped;
            }
            break;
        default:
            break;
    }

    PROC_process_channels(process_channel_mode,
        ma_maybe_next_planned_mode,
        ma_maybe_next_planned_delay == -1 ? std::optional<size_t>(std::nullopt) : ma_maybe_next_planned_delay.load(),
        ma_maybe_next_planned_delay == 0 ? PROC_predicted_next_trigger_eta() : std::nullopt,
        n_samples, pos_before, pos_after,
        length_before, length_after);

    if (mp_next_poi) { mp_next_poi.value().when -= n_samples; }
    ma_position = pos_after;
    ma_length = length_after;
    if (mp_next_trigger.has_value()) {
        mp_next_trigger = (size_t)(std::max(0, (int)mp_next_trigger.value() - (int)n_samples));
        if (mp_next_trigger.value() == 0) { mp_next_trigger = std::nullopt; }
    }
    PROC_handle_poi();
}

void BasicLoop::set_sync_source(std::shared_ptr<LoopInterface> const& src, bool thread_safe) {
    auto fn = [=, this]() {
        mp_sync_source = src;
        PROC_update_trigger_eta();
    };
    if(thread_safe) {
        exec_process_thread_command(fn);
    } else {
        fn();
    }
}
std::shared_ptr<LoopInterface> BasicLoop::get_sync_source(bool thread_safe) {
    if(thread_safe) {
        std::shared_ptr<LoopInterface> rval;
        exec_process_thread_command([this, &rval]() { rval = mp_sync_source; });
        return rval;
    }
    return mp_sync_source;
}

void BasicLoop::PROC_update_planned_transition_cache() {
    ma_maybe_next_planned_mode = mp_planned_states.size() > 0 ?
        (loop_mode_t) mp_planned_states.front() : LOOP_MODE_INVALID;
    ma_maybe_next_planned_delay = mp_planned_state_countdowns.size() > 0 ?
        mp_planned_state_countdowns.front() : -1;
}

void BasicLoop::PROC_trigger(bool propagate) {
    if (ma_already_triggered) { return; }
    ma_already_triggered = true;
    
    if (propagate) {
        ma_triggering_now = true;
    }

    if (is_playing_mode(ma_mode) && ma_position >= ma_length) {
        ma_position = 0;
    }

    for (auto &elem: mp_planned_state_countdowns) {
        elem--;
    }
    while (mp_planned_state_countdowns.size() > 0 && mp_planned_state_countdowns.front() < 0) {
        PROC_handle_transition(mp_planned_states.front());
        mp_planned_state_countdowns.pop_front();
        mp_planned_states.pop_front();
    }
    PROC_update_planned_transition_cache();
}

void BasicLoop::PROC_handle_sync() {
    if (mp_sync_source && mp_sync_source->PROC_is_triggering_now()) {
        PROC_trigger();
    }
}

void BasicLoop::PROC_handle_transition(loop_mode_t new_state) {
    if (ma_mode != new_state) {
        bool from_playing_to_playing = is_playing_mode(ma_mode) && is_playing_mode(new_state);
        if (!from_playing_to_playing) {
            set_position(0, false);
        }
        if (new_state == Recording) {
            // Recording always resets the loop.
            // Don't bother clearing the channels.
            set_length(0, false);
        }
        ma_mode = new_state;
        if(ma_mode > LOOP_MODE_INVALID) {
            throw std::runtime_error ("Invalid mode");
        }
        if (ma_mode == Stopped) { ma_position = 0; }
        if (is_playing_mode(ma_mode) &&
            ma_position == 0) {
                ma_triggering_now = true;
            }
        mp_next_poi = std::nullopt;
        PROC_update_poi();
        PROC_update_trigger_eta();
    }
}

size_t BasicLoop::get_n_planned_transitions(bool thread_safe) {
    if (thread_safe) {
        size_t rval;
        exec_process_thread_command([this, &rval]() { rval = mp_planned_states.size(); });
        return rval;
    }
    return mp_planned_states.size();
}

size_t BasicLoop::get_planned_transition_delay(size_t idx, bool thread_safe) {
    size_t rval;
    auto fn = [this,idx,&rval]() {
        if(idx >= mp_planned_state_countdowns.size()) {
            throw std::runtime_error("Attempted to get out-of-bounds planned transition");
        }
        rval = mp_planned_state_countdowns.at(idx);
    };
    if (thread_safe) {
        exec_process_thread_command(fn);
    } else {
        fn();
    }
    return rval;
}

loop_mode_t BasicLoop::get_planned_transition_state(size_t idx, bool thread_safe) {
    loop_mode_t rval;
    auto fn = [this, idx, &rval]() {
        if(idx >= mp_planned_states.size()) {
            throw std::runtime_error("Attempted to get out-of-bounds planned transition");
        }
        rval = mp_planned_states.at(idx);
    };
    if (thread_safe) {
        exec_process_thread_command(fn);
    } else {
        fn();
    }
    return rval;
}

void BasicLoop::clear_planned_transitions(bool thread_safe) {
    auto fn = [this]() { 
        mp_planned_states.clear();
        mp_planned_state_countdowns.clear();
        PROC_update_planned_transition_cache();
    };
    if (thread_safe) {
        exec_process_thread_command(fn);
    } else {
        fn();
    }
}

void BasicLoop::plan_transition(loop_mode_t mode, size_t n_cycles_delay, bool wait_for_sync, bool thread_safe) {
    auto fn = [this, mode, wait_for_sync, n_cycles_delay]() {
        bool transitioning_immediately =
            (!mp_sync_source && ma_mode != Playing) ||
            (!wait_for_sync);
        if (transitioning_immediately) {
            // Un-synced loops transition immediately from non-playing
            // states.
            PROC_handle_transition(mode);
            mp_planned_states.clear();
            mp_planned_state_countdowns.clear();
        } else {
            size_t insertion_point;
            for (insertion_point=0; insertion_point <= mp_planned_state_countdowns.size(); insertion_point++) {
                if (insertion_point < mp_planned_state_countdowns.size() &&
                    mp_planned_state_countdowns[insertion_point] >= n_cycles_delay) { break; }
            }
            if (insertion_point >= mp_planned_state_countdowns.size()) {
                mp_planned_state_countdowns.push_back(n_cycles_delay);
                mp_planned_states.push_back(mode);
            } else {
                // Replace some planned states
                mp_planned_state_countdowns[insertion_point] = n_cycles_delay;
                mp_planned_states[insertion_point] = mode;
                mp_planned_state_countdowns.resize(insertion_point+1);
                mp_planned_states.resize(insertion_point+1);
            }
        }
        PROC_update_planned_transition_cache();
        PROC_update_trigger_eta();
    };
    if (thread_safe) { exec_process_thread_command(fn); }
    else { fn(); }
}

size_t BasicLoop::get_position() const {
    return ma_position;
}

void BasicLoop::set_position(size_t position, bool thread_safe) {
    auto fn = [this, position]() {
        if (position != ma_position) {
            mp_next_poi = std::nullopt;
            mp_next_trigger = std::nullopt;
            ma_position = position;
            PROC_update_poi();
            PROC_update_trigger_eta();
        }
    };
    if (thread_safe) { exec_process_thread_command(fn); }
    else { fn(); }
}

size_t BasicLoop::get_length() const {
    return ma_length;
}

loop_mode_t BasicLoop::get_mode() const {
    return ma_mode;
}

void BasicLoop::get_first_planned_transition(loop_mode_t &maybe_mode_out, size_t &delay_out) {
    loop_mode_t maybe_mode = ma_maybe_next_planned_mode;
    int maybe_delay = ma_maybe_next_planned_delay;
    if (maybe_delay >= 0 && maybe_mode != LOOP_MODE_INVALID) {
        maybe_mode_out = maybe_mode;
        delay_out = maybe_delay;
    } else {
        maybe_mode_out = LOOP_MODE_INVALID;
        delay_out = 0;
    }
}

void BasicLoop::set_length(size_t len, bool thread_safe) {
    auto fn = [this, len]() {
        if (len != ma_length) {
            ma_length = len;
            if (ma_position >= len) {
                set_position(len == 0 ? 0 : len-1, false);
            }
            mp_next_poi = std::nullopt;
            mp_next_trigger = std::nullopt;
            PROC_update_poi();
            PROC_update_trigger_eta();
        }
    };
    if (thread_safe) { exec_process_thread_command(fn); }
    else { fn(); }
}

void BasicLoop::set_mode(loop_mode_t mode, bool thread_safe) {
    auto fn = [this, mode]() {
        PROC_handle_transition(mode);
    };
    if (thread_safe) { exec_process_thread_command(fn); }
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