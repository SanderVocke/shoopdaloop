#pragma once
#include <boost/container/flat_set.hpp>
#include <array>
#include <functional>
#include "MidiStateTracker.h"
#include "shoop_shared_ptr.h"
#include "backend_rust/src/cxx.rs.h"

enum class StateDiffTrackerAction {
    ScanDiff,
    ClearDiff,
    None
};

class MidiStateDiffTracker : public shoop_enable_shared_from_this<MidiStateDiffTracker> {
    using SharedTracker = shoop_shared_ptr<MidiStateTracker>;
    using DifferingState = std::array<uint8_t, 2>;
    using DiffSet = boost::container::flat_set<DifferingState>;
    static const uint32_t default_diffs_size = 256;

    SharedTracker m_a = nullptr, m_b = nullptr;
    rust::Box<backend_rust::MidiStateDiffTracker> m_rust;

public:
    MidiStateDiffTracker();
    MidiStateDiffTracker(SharedTracker a, SharedTracker b, StateDiffTrackerAction action = StateDiffTrackerAction::ScanDiff);

    void reset(SharedTracker a = nullptr, SharedTracker b = nullptr, StateDiffTrackerAction action = StateDiffTrackerAction::ScanDiff);
    void add_diff(DifferingState a);
    void delete_diff(DifferingState a);
    void check_note(uint8_t channel, uint8_t note);
    void check_cc(uint8_t channel, uint8_t controller);
    void check_program(uint8_t channel);
    void check_channel_pressure(uint8_t channel);
    void check_pitch_wheel(uint8_t channel);
    void rescan_diff();
    void clear_diff();
    void resolve_to(MidiStateTracker *to, std::function<void(uint32_t size, uint8_t *data)> put_message_cb,
        bool notes = true, bool controls = true, bool programs = true);
    void resolve_to_a(std::function<void(uint32_t size, uint8_t *data)> put_message_cb, bool notes = true, bool controls = true, bool programs = true);
    void resolve_to_b(std::function<void(uint32_t size, uint8_t *data)> put_message_cb, bool notes = true, bool controls = true, bool programs = true);

    void set_diff(DiffSet const &diff);
    DiffSet get_diff() const;
    SharedTracker a() const;
    SharedTracker b() const;
};
