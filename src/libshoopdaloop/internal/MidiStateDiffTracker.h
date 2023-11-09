#pragma once
#include <boost/container/flat_set.hpp>
#include "MidiStateTracker.h"

enum class StateDiffTrackerAction {
    ScanDiff,
    ClearDiff,
    None
};

// MidiStateDiffTracker combines two MIDI state trackers and keeps track of the differences
// in state between them. It provides quick access to a shortlist of any notes/controls/programs
// that are not the same in both state trackers, so that it is not necessary to fully iterate
// over all state in both trackers to get this information.
class MidiStateDiffTracker : public MidiStateTracker::Subscriber, public std::enable_shared_from_this<MidiStateDiffTracker> {
    using SharedTracker = std::shared_ptr<MidiStateTracker>;

    // Two bytes identify the state, but don't hold its value. (example: two bytes of CC identify the channel and controller in question).
    // Special for notes is that they may be indicated by NoteOn or NoteOff but this distinction does not mean anything here
    // (example: 0x90, 127 signifies there is a diff on note 127 of channel 0, regardless of what the exact note states are)
    using DifferingState = std::array<uint8_t, 2>;
    
    using DiffSet = boost::container::flat_set<DifferingState>;

    static const uint32_t default_diffs_size = 256;

    SharedTracker m_a, m_b;
    DiffSet m_diffs;

    void note_changed(MidiStateTracker *tracker, uint8_t channel, uint8_t note, std::optional<uint8_t> maybe_velocity) override;
    void cc_changed(MidiStateTracker *tracker, uint8_t channel, uint8_t cc, uint8_t value) override;
    void program_changed(MidiStateTracker *tracker, uint8_t channel, uint8_t program) override;
    void pitch_wheel_changed(MidiStateTracker *tracker, uint8_t channel, uint16_t pitch) override;
    void channel_pressure_changed(MidiStateTracker *tracker, uint8_t channel, uint8_t pressure) override;
   
public:
    MidiStateDiffTracker();
    MidiStateDiffTracker(SharedTracker a, SharedTracker b, StateDiffTrackerAction action=StateDiffTrackerAction::ScanDiff);

    // (Re-)set the diffs to compare.
    void reset(SharedTracker a=nullptr, SharedTracker b=nullptr, StateDiffTrackerAction action=StateDiffTrackerAction::ScanDiff);
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
        bool notes=true, bool controls=true, bool programs=true);

    void resolve_to_a(std::function<void(uint32_t size, uint8_t *data)> put_message_cb, bool notes=true, bool controls=true, bool programs=true);
    void resolve_to_b(std::function<void(uint32_t size, uint8_t *data)> put_message_cb, bool notes=true, bool controls=true, bool programs=true);

    void set_diff(DiffSet const& diff);

    DiffSet const& get_diff() const;
    SharedTracker a() const;
    SharedTracker b() const;
};