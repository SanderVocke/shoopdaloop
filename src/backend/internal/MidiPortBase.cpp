#include "MidiPortBase.h"
#include "shoop_globals.h"

MidiPortBase::MidiPortBase(const TrackingConfig& config)
    : ModuleLoggingEnabled<"Backend.MidiPortBase">(),
      m_tracking_config(config)
{
    if (config.track_notes || config.track_controls || config.track_programs) {
        m_maybe_midi_state = shoop_make_shared<MidiStateTracker>(
            config.track_notes, config.track_controls, config.track_programs
        );
    }
    m_ringbuffer_tail_state = shoop_make_shared<MidiStateTracker>(
        config.track_notes, config.track_controls, config.track_programs
    );
}

MidiPortBase::MidiPortBase(bool track_notes, bool track_controls, bool track_programs)
    : MidiPortBase(TrackingConfig(track_notes, track_controls, track_programs))
{
}

MidiPortBase::~MidiPortBase() {}

// IMidiStateTracking implementation
uint32_t MidiPortBase::n_notes_active() const {
    auto& m = m_maybe_midi_state;
    return m ? m->n_notes_active() : 0;
}

uint32_t MidiPortBase::get_input_event_count() const {
    return n_input_events;
}

uint32_t MidiPortBase::get_output_event_count() const {
    return n_output_events;
}

// IMidiRingbuffer implementation
void MidiPortBase::set_n_samples(uint32_t n) {
    if (n > 0 && !m_midi_ringbuffer) {
        m_midi_ringbuffer = shoop_make_shared<MidiRingbuffer>(shoop_constants::midi_storage_size);
    }
    if (m_midi_ringbuffer) {
        m_midi_ringbuffer->set_n_samples(n);
    }
}

uint32_t MidiPortBase::get_n_samples() const {
    return m_midi_ringbuffer ? m_midi_ringbuffer->get_n_samples() : 0;
}

void MidiPortBase::snapshot(IMidiStorage& target, std::optional<uint32_t> start_offset_from_end) const {
    if (m_midi_ringbuffer) {
        m_midi_ringbuffer->snapshot(target, start_offset_from_end);
    } else {
        target.clear();
    }
}

uint32_t MidiPortBase::get_current_n_samples() const {
    return get_n_samples();
}

// State tracker accessors
shoop_shared_ptr<MidiStateTracker>& MidiPortBase::maybe_midi_state_tracker() {
    return m_maybe_midi_state;
}

const shoop_shared_ptr<MidiStateTracker>& MidiPortBase::maybe_midi_state_tracker() const {
    return m_maybe_midi_state;
}

shoop_shared_ptr<MidiStateTracker>& MidiPortBase::maybe_ringbuffer_tail_state_tracker() {
    return m_ringbuffer_tail_state;
}

const shoop_shared_ptr<MidiStateTracker>& MidiPortBase::maybe_ringbuffer_tail_state_tracker() const {
    return m_ringbuffer_tail_state;
}

// Ringbuffer accessors
shoop_shared_ptr<MidiRingbuffer>& MidiPortBase::maybe_midi_ringbuffer() {
    return m_midi_ringbuffer;
}

const shoop_shared_ptr<MidiRingbuffer>& MidiPortBase::maybe_midi_ringbuffer() const {
    return m_midi_ringbuffer;
}

// Event counter management
void MidiPortBase::reset_n_input_events() { n_input_events = 0; }
uint32_t MidiPortBase::get_n_input_events() const { return n_input_events; }
void MidiPortBase::increment_input_events(uint32_t count) { n_input_events += count; }

void MidiPortBase::reset_n_output_events() { n_output_events = 0; }
uint32_t MidiPortBase::get_n_output_events() const { return n_output_events; }
void MidiPortBase::increment_output_events(uint32_t count) { n_output_events += count; }

// State processing helpers
void MidiPortBase::process_msg_to_state(const uint8_t* data) {
    if (m_maybe_midi_state) {
        m_maybe_midi_state->process_msg(data);
    }
}

void MidiPortBase::snapshot_ringbuffer_into(IMidiStorage& s) const {
    if (m_midi_ringbuffer) {
        m_midi_ringbuffer->snapshot(s);
    } else {
        s.clear();
    }
}

void MidiPortBase::set_muted(bool muted) {
    m_muted = muted;
}

bool MidiPortBase::get_muted() const {
    return m_muted;
}