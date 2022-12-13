#include <jack/midiport.h>
#include <vector>
#include <optional>
#include <cstring>

typedef jack_midi_event_t midi_event_t;

typedef struct _midi_event_metadata_t {
    jack_nframes_t time;
    size_t         size;
} midi_event_metadata_t;

struct MIDIRingBuffer {
    std::vector<uint8_t> data;
    size_t n_events;
    size_t tail;
    size_t head;

    size_t bytes_available() {
        return (head >= tail) ?
            data.size() - (head - tail) :
            (tail - head) - 1;
    }

    midi_event_metadata_t* peek_metadata(size_t offset=0) {
        return n_events > 0 ?
            (midi_event_metadata_t*) &(data[tail]) :
            nullptr;
    }

    std::optional<size_t> next_event_offset(size_t offset) {
        auto metadata = peek_metadata(offset);
        if (metadata == nullptr) {
            return std::none;
        }
        auto new_offset = offset + sizeof(midi_event_metadata_t) + metadata->size;
        return new_offset < head ? new_offset : nullptr;
    }

    bool put (midi_event_t const& event) {
        if (bytes_available() < event.size) { return false; }

        memcpy(&(data[head]), &event, sizeof(midi_event_metadata_t) + event.size);
        head += event.size;
        return true;
    }
};