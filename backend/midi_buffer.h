#include <jack/midiport.h>
#include <vector>
#include <optional>
#include <cstring>
#include <iostream>

typedef jack_midi_event_t midi_event_t;

typedef struct _midi_event_metadata_t {
    jack_nframes_t time;
    uint32_t       size;
} midi_event_metadata_t;

constexpr size_t typical_midi_message_size =
    sizeof(midi_event_metadata_t) + 3;

struct MIDIRingBuffer {
    std::vector<uint8_t> data;
    size_t n_events;
    size_t tail; // Oldest event start
    size_t head; // Write position
    size_t head_start; // Youngest event start
    int32_t cursor;

    uint8_t *copy_bytes(unsigned *length_out) {
        *length_out = data.size() - bytes_available();
        uint8_t *rval = (uint8_t*)malloc((size_t)*length_out);
        for(size_t idx=0; idx < *length_out; idx++) {
            rval[idx] = data[(idx + tail) % data.size()];
        }
        return rval;
    }

    void adopt_bytes(uint8_t *bytes, unsigned length) {
        if (length > data.size()) {
            data.resize(length);
        }
        memcpy(data.data(), bytes, (size_t) length);
        tail = 0;
        head = (size_t) length;
        head_start = 0;
        n_events = 0;

        // Find start of last message
        while ((head_start + peek_metadata(head_start, true)->size + sizeof (midi_event_metadata_t)) < head) {
            head_start += (peek_metadata(head_start, true)->size + sizeof (midi_event_metadata_t));
            n_events++;
        }

        reset_cursor();
    }

    size_t bytes_available() {
        return data.size() - (normalize_idx(head) - normalize_idx(tail));
    }
    
    size_t normalize_idx(size_t idx) {
        return (idx >= tail) ? idx : 
            (data.size() - (tail - idx));
    }

    midi_event_metadata_t* peek_metadata(size_t data_byte_offset, bool ignore_n_events = false) {
        return (ignore_n_events || n_events > 0) ?
            (midi_event_metadata_t*) &(data[data_byte_offset]) :
            nullptr;
    }

    midi_event_metadata_t* peek_cursor_metadata() {
        if(cursor >= 0) { return peek_metadata(cursor); }
        return nullptr;
    }

    uint8_t *peek_cursor_event_data() {
        auto metadata = peek_cursor_metadata();
        if(!metadata) { return nullptr; }
        return &(data[cursor + sizeof(midi_event_metadata_t)]);
    }

    uint8_t *event_data(size_t event_start) {
        return &(data[event_start + sizeof(midi_event_metadata_t)]);
    }

    bool cursor_valid() { return cursor >= 0; }

    // Try to increment the cursor to the next position.
    // returns true if successful.
    // If unsuccessful, cursor remains.
    bool increment_cursor() {
        if(cursor == -1) { return false; }
        auto next = next_event_offset(cursor);
        if(next) { 
            cursor = next.value();
            return true;
        }
        return false;
    }

    // Reset cursor to the tail if any, to -1 otherwise.
    void reset_cursor() {
        cursor = (n_events == 0) ? -1 : tail;
    }

    std::optional<size_t> next_event_offset(size_t data_byte_offset) {
        auto metadata = peek_metadata(data_byte_offset);
        if (metadata == nullptr) {
            return {};
        }
        size_t new_offset = (data_byte_offset + sizeof(midi_event_metadata_t) + metadata->size) % data.size();
        return normalize_idx(new_offset) < normalize_idx(head) ? new_offset : std::optional<size_t>({});
    }

    bool put (midi_event_metadata_t metadata, uint8_t* data) {
        auto total_size = sizeof(metadata) + metadata.size;
        auto write_to = 0;
        if (bytes_available() < total_size) { return false; }
        
        auto maybe_head_metadata = peek_metadata(head_start);
        if((maybe_head_metadata && metadata.time > maybe_head_metadata->time) ||
           n_events == 0) {
            // Put at the end of the buffer
            write_to = head;
            head_start = head;
            head += sizeof(metadata) + metadata.size;
            memcpy(&(this->data[write_to]), &metadata, sizeof(metadata));
            memcpy(&(this->data[write_to+sizeof(metadata)]), data, metadata.size);
            n_events++;

            return true;
        }
        // Ignore otherwise
        return false;        
    }

    MIDIRingBuffer(size_t bytes) {
        this->data = std::vector<uint8_t>(bytes);
        this->n_events = 0;
        this->head = this->tail = this->head_start = 0;
        reset_cursor();
    }

    MIDIRingBuffer() {
        MIDIRingBuffer(0);
    }
};