#include <algorithm>
#include <jack/midiport.h>
#include <iostream>
#include <chrono>
#include <optional>
#include <vector>
#include <algorithm>

struct ActiveNote {
    std::chrono::high_resolution_clock::time_point on_t;
    unsigned channel;
    unsigned note;
    unsigned velocity;
};

struct NoteOnOff {
    std::chrono::high_resolution_clock::time_point on_t;
    std::chrono::high_resolution_clock::time_point off_t;
    unsigned channel;
    unsigned note;
    unsigned velocity;
};

unsigned channel (jack_midi_event_t &msg) {
    return msg.buffer[0] & 0x0F;
}

unsigned note (jack_midi_event_t &msg) {
    return msg.buffer[1];
}

unsigned velocity (jack_midi_event_t &msg) {
    return msg.buffer[2];
}

struct MIDINotesState {
    MIDINotesState() {
        active_notes.reserve(16*128);
        last_notes.reserve(16*128);
        reset();
    }

    void reset() {
        active_notes.clear();
        last_notes.clear();
    }

    void sort_last_notes() {
        std::sort(last_notes.begin(), last_notes.end(), [](auto &a, auto &b) { return a.on_t < b.on_t; });
    }

    void sort_active_notes() {
        std::sort(last_notes.begin(), last_notes.end(), [](auto &a, auto &b) { return a.on_t < b.on_t; });
    }

    void sort() {
        sort_last_notes();
        sort_active_notes();
    }

    std::vector<ActiveNote> active_notes;
    std::vector<NoteOnOff>   last_notes;
};

// Tracks state of a single MIDI port regarding active and last MIDI notes.
// Notes will always be sorted according to start time.
struct MIDIStateTracker : public MIDINotesState {
    MIDIStateTracker() : MIDINotesState() {}

    bool is_noteOn(jack_midi_event_t &msg) {
        return (msg.buffer[0] & 0xF0) == 0x90;
    }

    bool is_noteOff(jack_midi_event_t &msg) {
        return (msg.buffer[0] & 0xF0) == 0x80;
    }

    void process_msg(jack_midi_event_t &msg) {
        if(is_noteOff(msg)) {
            // Move from active to last, otherwise ignore
            auto it = std::find_if(active_notes.begin(), active_notes.end(),
                [&msg](ActiveNote &a) {
                    return a.channel = channel(msg) && a.note == note(msg);
                    });
            if (it != active_notes.end()) {
                std::erase_if(last_notes,
                [&msg](NoteOnOff &a) {
                    return a.channel = channel(msg) && a.note == note(msg);
                    });
                last_notes.push_back({
                    .on_t = it->on_t,
                    .channel = it->channel,
                    .note = it->note,
                    .velocity = it->velocity
                });
                active_notes.erase(it);
                sort_last_notes();
            }
        } else if(is_noteOn(msg)) {
            // Remove from active notes in case it is a double noteon
            std::erase_if(active_notes,
                [&msg](ActiveNote &a) {
                    return a.channel = channel(msg) && a.note == note(msg);
                    });
            active_notes.push_back({.channel = channel(msg), .note = note(msg), .velocity = velocity(msg)});
            sort_active_notes();
        }
    }
};