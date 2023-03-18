#pragma once
#include <vector>
#include <cstddef>
#include <cstdint>
#include <optional>
#include <atomic>

size_t channel(const uint8_t *msg_data) {
    return msg_data[0] & 0x0F;
}

size_t note (const uint8_t *msg_data) {
    return msg_data[1];
}

size_t velocity (const uint8_t *msg_data) {
    return msg_data[2];
}

bool is_noteOn(const uint8_t *msg_data) {
    return (msg_data[0] & 0xF0) == 0x90;
}

bool is_noteOff(const uint8_t *msg_data) {
    return (msg_data[0] & 0xF0) == 0x80;
}

bool is_cc(const uint8_t *msg_data) {
    return (msg_data[0] & 0xF0) == 0xB0;
}

std::optional<size_t> is_all_notes_off_for_channel(const uint8_t *msg_data) {
    if (is_cc(msg_data) && msg_data[1] == 123) {
        return channel(msg_data);
    }
    return std::nullopt;
}

std::optional<size_t> is_all_sound_off_for_channel(const uint8_t *msg_data) {
    if (is_cc(msg_data) && msg_data[1] == 120) {
        return channel(msg_data);
    }
    return std::nullopt;
}

struct MidiNotesState {
    std::atomic<int> m_n_notes_active;
    std::vector<bool> m_notes_active;

    MidiNotesState() :
        m_n_notes_active(0),
        m_notes_active(16*128)
        {
            for (size_t i=0; i<m_notes_active.size(); i++) {
                m_notes_active[i] = false;
            }
        }

    void process_noteOn(size_t channel, size_t note) {
        auto idx = channel * 128 + note;
        if (!m_notes_active[idx]) {
            m_notes_active[idx] = true;
            m_n_notes_active++;
        }
    }

    void process_noteOff(size_t channel, size_t note) {
        auto idx = channel * 128 + note;
        if (m_notes_active[idx]) {
            m_notes_active[idx] = false;
            m_n_notes_active = std::max(0, m_n_notes_active-1);
        }
    }
    
    void process_msg(const uint8_t *data) {
        if (is_noteOn(data)) {
            auto _channel = channel(data);
            auto _note = note(data);
            process_noteOn(_channel, _note);
        } else if (is_noteOff(data)) {
            auto _channel = channel(data);
            auto _note = note(data);
            process_noteOff(_channel, _note);
        } else if (auto chan = is_all_notes_off_for_channel(data)) {
            for(size_t i=0; i<128; i++) {
                process_noteOff(*chan, i);
            }
        } else if (auto chan = is_all_sound_off_for_channel(data)) {
            for(size_t i=0; i<128; i++) {
                process_noteOff(*chan, i);
            }
        }
    }

    size_t n_notes_active() const { return m_n_notes_active; }
};