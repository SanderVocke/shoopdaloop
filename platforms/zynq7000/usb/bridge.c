#include "bridge.h"
#include <stdatomic.h>
#include <stdio.h>
#include <string.h>

struct event {
    uint64_t at;
    uint32_t generation, recovery_epoch;
    uint16_t len;
    uint8_t bytes[SHOOP_MIDI_BYTES];
};

struct port {
    struct shoop_usb_port_info info;
    _Atomic uint32_t sequence, published[sizeof(struct shoop_usb_port_info) / 4];
    _Atomic uint32_t state, head, tail, recovery;
    _Atomic uint32_t release_generation;
    _Atomic uint32_t received, completed, refused, dropped, malformed, assembly_dropped, recovery_requests, packets;
    struct event queue[SHOOP_MIDI_QUEUE];
    struct shoop_midi_decoder decoder;
    uint16_t offset;
    uint8_t reset_sent, reset_done;
    bool resetting;
    uint32_t input_epoch;
    uint8_t input_resets;
};

static struct port ports[2][SHOOP_MIDI_PORTS];
_Atomic uint32_t shoop_usb_consumed[SHOOP_MIDI_PORTS];
_Static_assert(sizeof(struct shoop_usb_port_info) == 280, "port information ABI");
_Static_assert(sizeof(ports) <= 32 * 1024 * 1024, "MIDI bridge exceeds 32 MiB");

static void publish(struct port *port) {
    atomic_fetch_add_explicit(&port->sequence, 1, memory_order_acq_rel);
    for (unsigned i = 0; i < sizeof(port->info) / 4; ++i) {
        uint32_t word;
        memcpy(&word, (const uint8_t *)&port->info + 4 * i, 4);
        atomic_store_explicit(&port->published[i], word, memory_order_relaxed);
    }
    atomic_fetch_add_explicit(&port->sequence, 1, memory_order_release);
}

bool shoop_usb_port_snapshot(unsigned direction, unsigned slot, struct shoop_usb_port_info *info) {
    if (direction > 1 || slot >= SHOOP_MIDI_PORTS || !info) return false;
    struct port *port = &ports[direction][slot];
    uint32_t before = atomic_load_explicit(&port->sequence, memory_order_acquire);
    if (before & 1) return false;
    for (unsigned i = 0; i < sizeof(*info) / 4; ++i) {
        uint32_t word = atomic_load_explicit(&port->published[i], memory_order_relaxed);
        memcpy((uint8_t *)info + 4 * i, &word, 4);
    }
    return before == atomic_load_explicit(&port->sequence, memory_order_acquire);
}

static bool enqueue(struct port *port, const uint8_t *bytes, size_t len, uint64_t at) {
    uint32_t state = atomic_load_explicit(&port->state, memory_order_acquire);
    if (!(state & 1) || !shoop_midi_valid(bytes, len)) {
        atomic_fetch_add(&port->refused, 1);
        return false;
    }
    uint32_t head = atomic_load_explicit(&port->head, memory_order_relaxed);
    if (head - atomic_load_explicit(&port->tail, memory_order_acquire) >= SHOOP_MIDI_QUEUE) {
        atomic_fetch_add(&port->dropped, 1);
        return false;
    }
    struct event *event = &port->queue[head % SHOOP_MIDI_QUEUE];
    event->at = at; event->generation = state >> 1; event->len = len;
    event->recovery_epoch = atomic_load_explicit(&port->recovery, memory_order_acquire);
    memcpy(event->bytes, bytes, len);
    atomic_store_explicit(&port->head, head + 1, memory_order_release);
    atomic_fetch_add(&port->received, 1);
    return true;
}

static struct event *front(struct port *port) {
    uint32_t tail = atomic_load_explicit(&port->tail, memory_order_relaxed);
    return tail == atomic_load_explicit(&port->head, memory_order_acquire) ? NULL :
        &port->queue[tail % SHOOP_MIDI_QUEUE];
}

static void pop(struct port *port) {
    atomic_fetch_add_explicit(&port->tail, 1, memory_order_release);
}

static int find_slot(unsigned direction, const char *key) {
    int free_slot = -1;
    for (unsigned i = 0; i < SHOOP_MIDI_PORTS; ++i) {
        struct port *port = &ports[direction][i];
        if (!strcmp(port->info.key, key)) return port->info.generation < 0x7fffffff ? (int)i : -1;
        if (!port->info.key[0] && port->info.generation < 0x7fffffff && free_slot < 0) free_slot = i;
    }
    return free_slot;
}

bool shoop_usb_mount(uint8_t index, uint8_t address, uint8_t interface_number,
                     uint8_t rx, uint8_t tx, uint16_t vendor, uint16_t product,
                     uint8_t speed, const char *key, const char *name) {
    if (index >= 8 || (!rx && !tx) || rx > 16 || tx > 16 || !key || !*key || !name || strlen(key) > 96 || strlen(name) > 96) return false;
    int slots[2][16];
    char port_key[128];
    for (unsigned direction = 0; direction < 2; ++direction) {
        unsigned count = direction ? tx : rx, needed = 0, free_count = 0;
        for (unsigned i = 0; i < SHOOP_MIDI_PORTS; ++i)
            free_count += !ports[direction][i].info.key[0] && ports[direction][i].info.generation < 0x7fffffff;
        for (unsigned cable = 0; cable < count; ++cable) {
            snprintf(port_key, sizeof(port_key), "%s/%s/%u", key, direction ? "out" : "in", cable);
            int slot = find_slot(direction, port_key);
            if (slot < 0 || ports[direction][slot].info.connected) return false;
            if (!ports[direction][slot].info.key[0]) ++needed;
        }
        if (needed > free_count) return false;
    }
    for (unsigned direction = 0; direction < 2; ++direction) {
        unsigned count = direction ? tx : rx;
        for (unsigned cable = 0; cable < count; ++cable) {
            snprintf(port_key, sizeof(port_key), "%s/%s/%u", key, direction ? "out" : "in", cable);
            int slot = find_slot(direction, port_key);
            if (slot < 0) return false;
            slots[direction][cable] = slot;
            struct port *port = &ports[direction][slot];
            ++port->info.generation;
            port->info.connected = 1; port->info.direction = direction;
            port->info.vendor = vendor; port->info.product = product;
            port->info.slot = slot; port->info.index = index; port->info.address = address;
            port->info.interface_number = interface_number; port->info.cable = cable; port->info.speed = speed;
            snprintf(port->info.key, sizeof(port->info.key), "%s", port_key);
            snprintf(port->info.name, sizeof(port->info.name), "%s %s %u", name, direction ? "out" : "in", cable + 1);
            memset(&port->decoder, 0, sizeof(port->decoder));
            port->offset = port->reset_sent = port->reset_done = 0; port->resetting = false;
            if (direction) atomic_store(&port->recovery, 0);
            publish(port);
        }
    }
    for (unsigned direction = 0; direction < 2; ++direction) {
        unsigned count = direction ? tx : rx;
        for (unsigned cable = 0; cable < count; ++cable) {
            struct port *port = &ports[direction][slots[direction][cable]];
            atomic_store_explicit(&port->state, port->info.generation * 2 + 1, memory_order_release);
        }
    }
    return true;
}

void shoop_usb_unmount(uint8_t index) {
    for (unsigned direction = 0; direction < 2; ++direction) {
        for (unsigned i = 0; i < SHOOP_MIDI_PORTS; ++i) {
            struct port *port = &ports[direction][i];
            if (port->info.index != index || !port->info.connected) continue;
            port->info.connected = 0;
            atomic_store_explicit(&port->state, port->info.generation * 2, memory_order_release);
            if (direction) atomic_store(&port->recovery, 0);
            else atomic_fetch_add(&port->recovery, 1);
            memset(&port->decoder, 0, sizeof(port->decoder));
            publish(port);
        }
    }
}

static bool receive_message(void *context, uint64_t now, const uint8_t *bytes, size_t len) {
    struct port *port = context;
    bool result = enqueue(port, bytes, len, now);
    if (!result) atomic_fetch_add(&port->recovery, 1);
    return result;
}

void shoop_usb_receive_packet(uint8_t index, const uint8_t packet[4], uint64_t now) {
    for (unsigned i = 0; i < SHOOP_MIDI_PORTS; ++i) {
        struct port *port = &ports[0][i];
        if (port->info.connected && port->info.index == index && port->info.cable == packet[0] >> 4) {
            uint32_t refused = port->decoder.refused, dropped = port->decoder.dropped;
            shoop_midi_decode(&port->decoder, packet, now, receive_message, port);
            atomic_fetch_add(&port->malformed, port->decoder.refused - refused);
            if (port->decoder.refused != refused) atomic_fetch_add(&port->recovery, 1);
            atomic_fetch_add(&port->assembly_dropped, port->decoder.dropped - dropped);
            return;
        }
    }
}

size_t shoop_usb_read(unsigned slot, uint8_t *bytes, size_t capacity, uint64_t *arrival) {
    if (slot >= SHOOP_MIDI_PORTS || !bytes || !arrival) return 0;
    struct port *port = &ports[0][slot];
    uint32_t epoch = atomic_load_explicit(&port->recovery, memory_order_acquire);
    if (epoch != port->input_epoch) {
        port->input_epoch = epoch;
        port->input_resets = 32;
    }
    if (port->input_resets) {
        if (capacity < 3) return 0;
        unsigned reset = 32 - port->input_resets--;
        bytes[0] = 0xb0 | (reset / 2);
        bytes[1] = reset % 2 ? 123 : 120;
        bytes[2] = 0;
        *arrival = 0;
        return 3;
    }
    for (unsigned i = 0; i < SHOOP_MIDI_QUEUE; ++i) {
        struct event *event = front(port);
        if (!event) return 0;
        uint32_t state = atomic_load_explicit(&port->state, memory_order_acquire);
        if (!(state & 1) || event->generation != state >> 1 || event->recovery_epoch != epoch) {
            pop(port);
            continue;
        }
        if (event->len > capacity) return 0;
        size_t len = event->len;
        memcpy(bytes, event->bytes, len); *arrival = event->at;
        pop(port);
        if (state != atomic_load_explicit(&port->state, memory_order_acquire)) return 0;
        atomic_fetch_add(&shoop_usb_consumed[slot], 1);
        return len;
    }
    return 0;
}

bool shoop_usb_write(unsigned slot, const uint8_t *bytes, size_t len, uint64_t due) {
    if (slot >= SHOOP_MIDI_PORTS) return false;
    struct port *port = &ports[1][slot];
    if (atomic_load(&port->recovery)) {
        atomic_fetch_add(&port->refused, 1);
        return false;
    }
    if (enqueue(port, bytes, len, due)) return true;
    if (atomic_load(&port->state) & 1) atomic_store(&port->recovery, 1);
    return false;
}

uint8_t shoop_usb_output_index(unsigned slot) {
    if (slot >= SHOOP_MIDI_PORTS || !ports[1][slot].info.connected) return UINT8_MAX;
    return ports[1][slot].info.index;
}

bool shoop_usb_prepare_packet(unsigned slot, uint64_t now, uint8_t packet[4],
                              struct shoop_usb_packet_owner *owner) {
    if (slot >= SHOOP_MIDI_PORTS) return false;
    struct port *port = &ports[1][slot];
    uint32_t state = atomic_load(&port->state);
    if (!(state & 1)) return false;
    *owner = (struct shoop_usb_packet_owner){.slot = slot, .generation = state >> 1};
    if (atomic_load(&port->recovery)) {
        if (!port->resetting) {
            for (unsigned i = 0; i < SHOOP_MIDI_QUEUE && front(port); ++i) pop(port);
            port->reset_sent = port->reset_done = port->offset = 0;
            port->resetting = true;
        }
        if (port->reset_sent >= 32) return false;
        packet[0] = (uint8_t)((port->info.cable << 4) | 11);
        packet[1] = 0xb0 | (port->reset_sent / 2);
        packet[2] = port->reset_sent % 2 ? 123 : 120;
        packet[3] = 0;
        owner->recovery = 1;
        return true;
    }
    for (unsigned i = 0; i < SHOOP_MIDI_QUEUE; ++i) {
        struct event *event = front(port);
        if (!event) return false;
        if (event->generation != state >> 1) { pop(port); port->offset = 0; continue; }
        if (event->at > now) return false;
        size_t bytes = shoop_midi_packet(event->bytes, event->len, port->offset, port->info.cable, packet);
        if (!bytes) { pop(port); port->offset = 0; continue; }
        owner->reserved = bytes;
        owner->last = port->offset + bytes == event->len;
        return true;
    }
    return false;
}

void shoop_usb_accept_packet(struct shoop_usb_packet_owner owner) {
    if (owner.slot >= SHOOP_MIDI_PORTS) return;
    struct port *port = &ports[1][owner.slot];
    if (owner.generation * 2 + 1 != atomic_load(&port->state)) return;
    if (owner.recovery) ++port->reset_sent;
    else if (owner.last) { pop(port); port->offset = 0; }
    else port->offset += owner.reserved;
}

void shoop_usb_complete_packet(struct shoop_usb_packet_owner owner) {
    if (owner.slot >= SHOOP_MIDI_PORTS) return;
    struct port *port = &ports[1][owner.slot];
    if (owner.generation * 2 + 1 != atomic_load(&port->state)) return;
    atomic_fetch_add(&port->packets, 1);
    if (owner.recovery && port->resetting) {
        if (++port->reset_done == 32) {
            port->resetting = false;
            atomic_store(&port->recovery, 0);
        }
    } else if (owner.last) atomic_fetch_add(&port->completed, 1);
}

void shoop_usb_recover_all(void) {
    for (unsigned i = 0; i < SHOOP_MIDI_PORTS; ++i) {
        shoop_usb_recover_port(i);
    }
}

void shoop_usb_recover_port(unsigned slot) {
    if (slot < SHOOP_MIDI_PORTS && (atomic_load(&ports[1][slot].state) & 1)) {
        atomic_fetch_add(&ports[1][slot].recovery_requests, 1);
        atomic_store(&ports[1][slot].recovery, 1);
    }
}

bool shoop_usb_drained(void) {
    for (unsigned i = 0; i < SHOOP_MIDI_PORTS; ++i) {
        struct port *port = &ports[1][i];
        if ((atomic_load(&port->state) & 1) && (front(port) || atomic_load(&port->recovery))) return false;
    }
    return true;
}

void shoop_usb_expire(uint64_t now) {
    for (unsigned i = 0; i < SHOOP_MIDI_PORTS; ++i) {
        struct port *port = &ports[0][i];
        uint32_t refused = port->decoder.refused;
        shoop_midi_expire(&port->decoder, now);
        atomic_fetch_add(&port->malformed, port->decoder.refused - refused);
        if (port->decoder.refused != refused) atomic_fetch_add(&port->recovery, 1);
    }
    for (unsigned direction = 0; direction < 2; ++direction) {
        for (unsigned i = 0; i < SHOOP_MIDI_PORTS; ++i) {
            struct port *port = &ports[direction][i];
            uint32_t generation = atomic_exchange(&port->release_generation, 0);
            if (!generation || generation != port->info.generation || port->info.connected) continue;
            port->info.key[0] = port->info.name[0] = 0;
            if (direction) {
                for (unsigned n = 0; n < SHOOP_MIDI_QUEUE && front(port); ++n) pop(port);
                port->offset = 0;
            }
            publish(port);
        }
    }
}

void shoop_usb_release(unsigned direction, unsigned slot, uint32_t generation) {
    if (direction < 2 && slot < SHOOP_MIDI_PORTS) atomic_store(&ports[direction][slot].release_generation, generation);
}

bool shoop_usb_counters(unsigned direction, unsigned slot, struct shoop_usb_counters *counters) {
    if (direction > 1 || slot >= SHOOP_MIDI_PORTS || !counters) return false;
    struct port *port = &ports[direction][slot];
    *counters = (struct shoop_usb_counters){
        .received = atomic_load(&port->received), .completed = atomic_load(&port->completed),
        .refused = atomic_load(&port->refused), .dropped = atomic_load(&port->dropped),
        .malformed = atomic_load(&port->malformed), .assembly_dropped = atomic_load(&port->assembly_dropped),
        .recovery_requests = atomic_load(&port->recovery_requests), .packets = atomic_load(&port->packets),
        .queued = atomic_load(&port->head) - atomic_load(&port->tail)};
    return true;
}
