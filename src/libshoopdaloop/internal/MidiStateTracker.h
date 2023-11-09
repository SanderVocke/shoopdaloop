#pragma once
#include <set>
#include "LoggingEnabled.h"

// If you feed MidiStateTracker a sequence of MIDI messages, it will keep track of current state
// on the MIDI stream, optionally including:
// - Notes on or off (including if terminated by All Sound Off / All Notes Off, but not including hold pedals)
// - CC values
// - Progam changes
// - Pitch wheel and channel pressure
class MidiStateTracker : public ModuleLoggingEnabled {
public:
    class Subscriber {
    public:
        virtual void note_changed(MidiStateTracker *from, uint8_t channel, uint8_t note, std::optional<uint8_t> maybe_velocity) = 0;
        virtual void cc_changed(MidiStateTracker *from, uint8_t channel, uint8_t cc, uint8_t value) = 0;
        virtual void program_changed(MidiStateTracker *from, uint8_t channel, uint8_t program) = 0;
        virtual void pitch_wheel_changed(MidiStateTracker *from, uint8_t channel, uint16_t pitch) = 0;
        virtual void channel_pressure_changed(MidiStateTracker *from, uint8_t channel, uint8_t pressure) = 0;
    };

private:
    std::atomic<int> m_n_notes_active;
    std::vector<std::optional<uint8_t>> m_notes_active_velocities;        // Track Note On / Note Off (16*128)
    std::vector<uint8_t> m_controls;                       // Track CC values          (128*128)
    std::vector<uint8_t> m_programs;                       // Track Program values     (16)
    std::vector<uint16_t> m_pitch_wheel;                    // 16 channels
    std::vector<uint8_t> m_channel_pressure;               // 16 channels
    std::set<std::weak_ptr<Subscriber>, std::owner_less<std::weak_ptr<Subscriber>>> m_subscribers;


    static uint32_t note_index (uint8_t channel, uint8_t note);
    static uint32_t cc_index (uint8_t channel, uint8_t cc);
    void process_noteOn(uint8_t channel, uint8_t note, uint8_t velocity);
    void process_noteOff(uint8_t channel, uint8_t note);
    void process_cc(uint8_t channel, uint8_t controller, uint8_t value);
    void process_program(uint8_t channel, uint8_t value);
    void process_pitch_wheel(uint8_t channel, uint16_t value);
    void process_channel_pressure(uint8_t channel, uint8_t value);
    std::string log_module_name() const override;

public:
    MidiStateTracker(bool track_notes=false, bool track_controls=false, bool track_programs=false);
    MidiStateTracker& operator= (MidiStateTracker const& other);

    void copy_relevant_state(MidiStateTracker const& other);
    void clear();

    void process_msg(const uint8_t *data);

    bool tracking_notes() const;
    uint32_t n_notes_active() const;
    std::optional<uint8_t> maybe_current_note_velocity(uint8_t channel, uint8_t note) const;

    bool tracking_controls() const;
    uint8_t cc_value (uint8_t channel, uint8_t controller);
    uint16_t pitch_wheel_value (uint8_t channel);
    uint8_t channel_pressure_value (uint8_t channel);

    bool tracking_programs() const;
    uint8_t program_value (uint8_t channel);

    bool tracking_anything() const;

    void subscribe(std::shared_ptr<Subscriber> s);
    void unsubscribe(std::shared_ptr<Subscriber> s);
};