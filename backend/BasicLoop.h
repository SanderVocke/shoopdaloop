#pragma once
#include "LoopInterface.h"
#include <cstring>
#include <memory>
#include <stdexcept>
#include <vector>
#include <optional>
#include <iostream>
#include <deque>

enum BasicPointOfInterestFlags {
    Trigger = 1,
    LoopEnd = 2,
    BasicPointOfInterestFlags_End = 4,
};

// The basic loop implements common semantics shared by all loop types.
// That includes keeping track of the first upcoming point of interest,
// logic of handling points of interest while processing, planning
// state transitions etc.
class BasicLoop : public LoopInterface {
public:
    struct PointOfInterest {
        size_t when;
        unsigned type_flags; // To be defined per implementation
    };

protected:
    std::optional<PointOfInterest> m_next_poi;
    std::shared_ptr<LoopInterface> m_sync_source;
    loop_state_t m_state;
    std::deque<loop_state_t> m_planned_states;
    std::deque<size_t> m_planned_state_countdowns;
    bool m_triggering_now;
    size_t m_length;
    size_t m_position;

public:

    BasicLoop() :
        m_next_poi(std::nullopt),
        m_state(Stopped),
        m_sync_source(nullptr),
        m_triggering_now(false),
        m_length(0),
        m_position(0)
    {}
    ~BasicLoop() override {}

    // We always assume that the POI has been properly updated by whatever calls were
    // made in the past. Simply return.
    std::optional<size_t> get_next_poi() const override {
        return m_next_poi ? m_next_poi.value().when : (std::optional<size_t>)std::nullopt;
    }

    virtual void update_poi() {
        if(m_next_poi) {
            m_next_poi->type_flags &= ~(LoopEnd);
            if (m_next_poi->type_flags == 0) {
                m_next_poi = std::nullopt;
            }
        }

        std::optional<PointOfInterest> loop_end_poi;
        if (m_state == Playing || m_state == PlayingMuted) {
            std::optional<PointOfInterest> loop_end_poi = PointOfInterest {
                .when = m_length - m_position,
                .type_flags = LoopEnd
            };
            m_next_poi = dominant_poi(m_next_poi, loop_end_poi);
        }
    }

    void handle_poi() override {
        // Only handle POIs that are reached right now.
        if (!m_next_poi || m_next_poi.value().when != 0) {
            return;
        }

        if (m_next_poi->type_flags & Trigger) {
            trigger();
            m_next_poi->type_flags &= ~(Trigger);
        }
        if (m_next_poi->type_flags & LoopEnd) {
            m_position = 0;
            m_triggering_now = true; // Emit a trigger on restart
            trigger();
            m_next_poi->type_flags &= !(LoopEnd);
        }

        if (m_next_poi->type_flags == 0) {
            m_next_poi = std::nullopt;
            update_poi();
        }
    }

    bool is_triggering_now() override {
        if (m_sync_source && m_sync_source->is_triggering_now()) { return true; }
        if (m_triggering_now) { return true; }
        if (m_next_poi.has_value() && m_next_poi.value().when == 0) {
            handle_poi();
            return is_triggering_now();
        }
        return false;
    }

    virtual void process_body(
        size_t n_samples,
        size_t pos_before,
        size_t &pos_after,
        size_t length_before,
        size_t &length_after,
        loop_state_t state_before,
        loop_state_t &state_after
        ) {}

    void process(size_t n_samples) override {
        if (n_samples == 0) {
            throw std::runtime_error("Processing by 0");
        }
        if (m_next_poi && n_samples > m_next_poi.value().when) {
            throw std::runtime_error("Attempted to process loop beyond its next POI.");
        }
        m_triggering_now = false;

        size_t pos_before = m_position;
        size_t pos_after = m_position;
        size_t length_before = m_length;
        size_t length_after = m_length;
        loop_state_t state_before = m_state;
        loop_state_t state_after = m_state;

        switch(m_state) {
            case Recording:
                length_after += n_samples;
                break;
            case Playing:
            case PlayingMuted:
                pos_after += n_samples;
                break;
            default:
                break;
        }

        process_body(n_samples, pos_before, pos_after,
            length_before, length_after, state_before, state_after);

        if (m_next_poi) { m_next_poi.value().when -= n_samples; }
        m_position = pos_after;
        m_length = length_after;
        m_state = state_after;
        handle_poi();
        update_poi();
    }

    void set_sync_source(std::shared_ptr<LoopInterface> const& src) override {
        m_sync_source = src;
    }
    std::shared_ptr<LoopInterface> const& get_sync_source() const override {
        return m_sync_source;
    }

    void trigger() override {
        m_triggering_now = true;
        if (m_planned_states.size() > 0) {
            if (m_planned_state_countdowns.front() == 0) {
                handle_transition(m_planned_states.front());
                m_planned_states.pop_front();
                m_planned_state_countdowns.pop_front();
            } else {
                m_planned_state_countdowns.front()--;
            }
        }
    }

    void handle_sync() override {
        if (m_sync_source && m_sync_source->is_triggering_now()) {
            trigger();
        }
    }

    void handle_transition(loop_state_t new_state) {
        m_state = new_state;
        if (m_state == Stopped) { m_position = 0; }
        if ((m_state == Playing || m_state == PlayingMuted) &&
            m_position == 0) {
                m_triggering_now = true;
            }
        m_next_poi = std::nullopt;
        update_poi();
    }

    size_t get_n_planned_transitions() const override {
        return m_planned_states.size();
    }

    size_t get_planned_transition_delay(size_t idx) const override {
        if(idx >= m_planned_state_countdowns.size()) {
            throw std::runtime_error("Attempted to get out-of-bounds planned transition");
        }
        return m_planned_state_countdowns.at(idx);
    }

    loop_state_t get_planned_transition_state(size_t idx) const override {
        if(idx >= m_planned_states.size()) {
            throw std::runtime_error("Attempted to get out-of-bounds planned transition");
        }
        return m_planned_states.at(idx);
    }

    void clear_planned_transitions() override {
        m_planned_states.clear();
        m_planned_state_countdowns.clear();
    }

    void plan_transition(loop_state_t state, size_t n_cycles_delay = 0) override {
        m_planned_states.push_back(state);
        m_planned_state_countdowns.push_back(n_cycles_delay);
    }

    size_t get_position() const override {
        return m_position;
    }

    void set_position(size_t pos) override {
        m_position = pos;
        update_poi();
    }

    size_t get_length() const override {
        return m_length;
    }

    loop_state_t get_state() const override {
        return m_state;
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