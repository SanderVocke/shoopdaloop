#pragma once
#include "LoopInterface.h"
#include "WithCommandQueue.h"
#include "types.h"
#include <cstring>
#include <memory>
#include <stdexcept>
#include <vector>
#include <optional>
#include <iostream>
#include <deque>
#include <atomic>

enum BasicPointOfInterestFlags {
    Trigger = 1,
    LoopEnd = 2,
    BasicPointOfInterestFlags_End = 4,
};

// The basic loop implements common semantics shared by all loop types.
// That includes keeping track of the first upcoming point of interest,
// logic of handling points of interest while processing, planning
// mode transitions etc.
class BasicLoop : public LoopInterface,
                  protected WithCommandQueue<100, 1000, 1000> {
public:
    struct PointOfInterest {
        size_t when;
        unsigned type_flags; // To be defined per implementation
    };

protected:
    std::optional<PointOfInterest> mp_next_poi;
    std::shared_ptr<LoopInterface> mp_sync_source;
    std::deque<loop_mode_t> mp_planned_states;
    std::deque<int> mp_planned_state_countdowns;

    std::atomic<loop_mode_t> ma_mode;
    std::atomic<bool> ma_triggering_now;
    std::atomic<size_t> ma_length;
    std::atomic<size_t> ma_position;
    
    // Cached state for easy lock-free reading.
    std::atomic<loop_mode_t> ma_maybe_next_planned_mode;
    std::atomic<int> ma_maybe_next_planned_delay;

public:

    BasicLoop() :
        WithCommandQueue<100, 1000, 1000>(),
        mp_next_poi(std::nullopt),
        ma_mode(Stopped),
        mp_sync_source(nullptr),
        ma_triggering_now(false),
        ma_length(0),
        ma_position(0),
        ma_maybe_next_planned_mode(LOOP_MODE_INVALID),
        ma_maybe_next_planned_delay(-1)
    {}
    ~BasicLoop() override {}

    // We always assume that the POI has been properly updated by whatever calls were
    // made in the past. Simply return.
    std::optional<size_t> PROC_get_next_poi() const override {
        return mp_next_poi.has_value() ? mp_next_poi.value().when : (std::optional<size_t>)std::nullopt;
    }

    virtual void PROC_update_poi() {
        if((ma_mode == Playing) && 
           ma_length == 0) {
            PROC_handle_transition(Stopped);
        }

        if(mp_next_poi) {
            mp_next_poi->type_flags &= ~(LoopEnd);
            if (mp_next_poi->type_flags == 0) {
                mp_next_poi = std::nullopt;
            }
        }

        std::optional<PointOfInterest> loop_end_poi;
        if (ma_mode == Playing) {
            std::optional<PointOfInterest> loop_end_poi = PointOfInterest {
                .when = ma_length - ma_position,
                .type_flags = LoopEnd
            };
            mp_next_poi = dominant_poi(mp_next_poi, loop_end_poi);
        }
    }

    void PROC_handle_poi() override {
        // Only handle POIs that are reached right now.
        if (!mp_next_poi || mp_next_poi.value().when != 0) {
            return;
        }

        if (mp_next_poi->type_flags & Trigger) {
            PROC_trigger();
            mp_next_poi->type_flags &= ~(Trigger);
        }
        if (mp_next_poi->type_flags & LoopEnd) {
            ma_position = 0;
            ma_triggering_now = true; // Emit a trigger on restart
            PROC_trigger();
            mp_next_poi->type_flags &= !(LoopEnd);
        }

        if (mp_next_poi->type_flags == 0) {
            mp_next_poi = std::nullopt;
            PROC_update_poi();
        }
    }

    bool PROC_is_triggering_now() override {
        if (mp_next_poi.has_value() && mp_next_poi.value().when == 0) {
            PROC_handle_poi();
        }
        if (mp_sync_source && mp_sync_source->PROC_is_triggering_now()) { return true; }
        if (ma_triggering_now) { return true; }
        return false;
    }

    virtual void PROC_process_subloops(
        loop_mode_t mode_before,
        loop_mode_t mode_after,
        size_t n_samples,
        size_t pos_before,
        size_t pos_after,
        size_t length_before,
        size_t length_after
    ) {}

    void PROC_process(size_t n_samples) override {
        if (n_samples == 0) {
            throw std::runtime_error("Processing by 0");
        }
        if (mp_next_poi && n_samples > mp_next_poi.value().when) {
            throw std::runtime_error("Attempted to process loop beyond its next POI.");
        }
        PROC_handle_command_queue();

        ma_triggering_now = false;

        size_t pos_before = ma_position;
        size_t pos_after = ma_position;
        size_t length_before = ma_length;
        size_t length_after = ma_length;

        switch(ma_mode) {
            case Recording:
                length_after += n_samples;
                break;
            case Playing:
                pos_after += n_samples;
                break;
            default:
                break;
        }

        PROC_process_subloops(ma_mode, ma_mode, n_samples, pos_before, pos_after,
            length_before, length_after);

        if (mp_next_poi) { mp_next_poi.value().when -= n_samples; }
        ma_position = pos_after;
        set_length(length_after, false);
        PROC_handle_poi();
        PROC_update_poi();
    }

    void set_sync_source(std::shared_ptr<LoopInterface> const& src, bool thread_safe=true) override {
        if(thread_safe) {
            exec_process_thread_command([=]() { mp_sync_source = src; });
        } else {
            mp_sync_source = src;
        }
    }
    std::shared_ptr<LoopInterface> get_sync_source(bool thread_safe = true) override {
        if(thread_safe) {
            std::shared_ptr<LoopInterface> rval;
            exec_process_thread_command([this, &rval]() { rval = mp_sync_source; });
            return rval;
        }
        return mp_sync_source;
    }

    void PROC_update_planned_transition_cache() {
        ma_maybe_next_planned_mode = mp_planned_states.size() > 0 ?
            (loop_mode_t) mp_planned_states.front() : LOOP_MODE_INVALID;
        ma_maybe_next_planned_delay = mp_planned_state_countdowns.size() > 0 ?
            mp_planned_state_countdowns.front() : -1;
    }

    void PROC_trigger(bool propagate=true) override {
        if (propagate) {
            ma_triggering_now = true;
        }

        for (auto &elem: mp_planned_state_countdowns) {
            elem--;
        }
        while (mp_planned_state_countdowns.front() < 0 && mp_planned_state_countdowns.size() > 0) {
            PROC_handle_transition(mp_planned_states.front());
            mp_planned_state_countdowns.pop_front();
            mp_planned_states.pop_front();
        }
        PROC_update_planned_transition_cache();
    }

    void PROC_handle_sync() override {
        if (mp_sync_source && mp_sync_source->PROC_is_triggering_now()) {
            PROC_trigger();
        }
    }

    void PROC_handle_transition(loop_mode_t new_state) {
        if (ma_mode != new_state) {
            if (new_state == Recording) {
                // Recording always resets the loop.
                // Don't bother clearing the channels.
                set_length(0, false);
                set_position(0, false);
            }
            ma_mode = new_state;
            if(ma_mode > LOOP_MODE_INVALID) {
                throw std::runtime_error ("Invalid mode");
            }
            if (ma_mode == Stopped) { ma_position = 0; }
            if ((ma_mode == Playing) &&
                ma_position == 0) {
                    ma_triggering_now = true;
                }
            mp_next_poi = std::nullopt;
            PROC_update_poi();
        }
    }

    size_t get_n_planned_transitions(bool thread_safe=true) override {
        if (thread_safe) {
            size_t rval;
            exec_process_thread_command([this, &rval]() { rval = mp_planned_states.size(); });
            return rval;
        }
        return mp_planned_states.size();
    }

    size_t get_planned_transition_delay(size_t idx, bool thread_safe=true) override {
        size_t rval;
        auto fn = [=,&rval]() {
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

    loop_mode_t get_planned_transition_state(size_t idx, bool thread_safe=true) override {
        loop_mode_t rval;
        auto fn = [=,&rval]() {
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

    void clear_planned_transitions(bool thread_safe) override {
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

    void plan_transition(loop_mode_t mode, size_t n_cycles_delay = 0, bool wait_for_sync = true, bool thread_safe=true) override {
        auto fn = [=]() {
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
        };
        if (thread_safe) { exec_process_thread_command(fn); }
        else { fn(); }
    }

    size_t get_position() const override {
        return ma_position;
    }

    void set_position(size_t position, bool thread_safe=true) override {
        auto fn = [=]() {
            if (position != ma_position) {
                mp_next_poi = std::nullopt;
                ma_position = position;
                PROC_update_poi();
            }
        };
        if (thread_safe) { exec_process_thread_command(fn); }
        else { fn(); }
    }

    size_t get_length() const override {
        return ma_length;
    }

    loop_mode_t get_mode() const override {
        return ma_mode;
    }

    void get_first_planned_transition(loop_mode_t &maybe_mode_out, size_t &delay_out) override {
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

    void set_length(size_t len, bool thread_safe=true) override {
        auto fn = [=]() {
            if (len != ma_length) {
                ma_length = len;
                if (ma_position >= len) {
                    set_position(len-1, false);
                }
                mp_next_poi = std::nullopt;
                PROC_update_poi();
            }
        };
        if (thread_safe) { exec_process_thread_command(fn); }
        else { fn(); }
    }

    void set_mode(loop_mode_t mode, bool thread_safe=true) override {
        auto fn = [=]() {
            PROC_handle_transition(mode);
        };
        if (thread_safe) { exec_process_thread_command(fn); }
        else { fn(); }
    }

protected:
    static std::optional<PointOfInterest> dominant_poi(std::optional<PointOfInterest> const& a, std::optional<PointOfInterest> const& b) {
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

    static std::optional<PointOfInterest> dominant_poi(std::vector<std::optional<PointOfInterest>> const& pois) {
        if (pois.size() == 0) { return std::nullopt; }
        if (pois.size() == 1) { return pois[0]; }
        if (pois.size() == 2) { return dominant_poi(pois[0], pois[1]); }
        auto reduced = std::vector<std::optional<PointOfInterest>>(pois.begin()+1, pois.end());
        return dominant_poi(pois[0], dominant_poi(reduced));
    }
};