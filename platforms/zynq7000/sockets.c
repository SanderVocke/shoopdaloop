#include "FreeRTOS.h"
#include "task.h"
#include "lwip/sockets.h"
#include <stdatomic.h>
#include <errno.h>
#include <string.h>
#include "deployment/runtime.h"

#define SLOTS 24
#define CAPACITY 32768u
#define COPY_LIMIT 4096u
enum state { FREE, ACCEPTED, OPEN, CLOSING };
struct ring { _Atomic uint32_t read, written; unsigned char bytes[CAPACITY]; };
struct connection {
    _Atomic unsigned state, eof, failed;
    unsigned generation, listener;
    int fd;
    uint64_t close_at;
    struct ring receive, transmit;
};
static struct connection connections[SLOTS];
static int listeners[2] = { -1, -1 };
static _Atomic unsigned stopping, stopped;
extern uint64_t shoop_millis(void);

static size_t available(struct ring *ring) {
    return atomic_load_explicit(&ring->written, memory_order_acquire) - atomic_load_explicit(&ring->read, memory_order_acquire);
}
static size_t portion(uint32_t position, size_t size) {
    if (size > COPY_LIMIT) size = COPY_LIMIT;
    size_t contiguous = CAPACITY - position % CAPACITY;
    return size < contiguous ? size : contiguous;
}
static int again(void) { return errno == EAGAIN || errno == EWOULDBLOCK || errno == EINTR; }
static int nonblocking(int fd) {
    unsigned long enabled = 1;
    struct timeval timeout = { .tv_sec = 0, .tv_usec = 100000 };
    return lwip_ioctl(fd, FIONBIO, &enabled) ||
        lwip_setsockopt(fd, SOL_SOCKET, SO_SNDTIMEO, &timeout, sizeof(timeout));
}
static void broker(void *unused) {
    (void)unused;
    for (;;) {
        if (atomic_load(&stopping)) {
            for (unsigned slot = 0; slot < SLOTS; ++slot) {
                if (atomic_load(&connections[slot].state) != FREE) lwip_close(connections[slot].fd);
                atomic_store(&connections[slot].state, FREE);
            }
            for (unsigned listener = 0; listener < 2; ++listener) {
                if (listeners[listener] >= 0) lwip_close(listeners[listener]);
                listeners[listener] = -1;
            }
            atomic_store(&stopped, 1);
            vTaskDelete(NULL);
        }
        for (unsigned listener = 0; listener < 2; ++listener) {
            if (listeners[listener] < 0) continue;
            for (unsigned count = 0; count < 4; ++count) {
                int fd = lwip_accept(listeners[listener], NULL, NULL);
                if (fd < 0) break;
                int nodelay = 1;
                if (nonblocking(fd) || lwip_setsockopt(fd, IPPROTO_TCP, TCP_NODELAY, &nodelay, sizeof(nodelay))) {
                    lwip_close(fd); continue;
                }
                unsigned slot;
                for (slot = 0; slot < SLOTS; ++slot) {
                    struct connection *peer = &connections[slot];
                    if (atomic_load(&peer->state) != FREE) continue;
                    peer->fd = fd; peer->listener = listener;
                    peer->generation = peer->generation % 0x3fffff + 1;
                    atomic_store(&peer->receive.read, 0); atomic_store(&peer->receive.written, 0);
                    atomic_store(&peer->transmit.read, 0); atomic_store(&peer->transmit.written, 0);
                    atomic_store(&peer->eof, 0); atomic_store(&peer->failed, 0);
                    atomic_store_explicit(&peer->state, ACCEPTED, memory_order_release);
                    break;
                }
                if (slot == SLOTS) lwip_close(fd);
            }
        }
        for (unsigned slot = 0; slot < SLOTS; ++slot) {
            struct connection *peer = &connections[slot];
            unsigned state = atomic_load_explicit(&peer->state, memory_order_acquire);
            if (state == FREE) continue;
            size_t queued = available(&peer->transmit);
            if (queued && !atomic_load(&peer->failed)) {
                uint32_t read = atomic_load(&peer->transmit.read);
                int count = lwip_send(peer->fd, peer->transmit.bytes + read % CAPACITY, portion(read, queued), 0);
                if (count > 0) atomic_store_explicit(&peer->transmit.read, read + count, memory_order_release);
                else if (count == 0 || !again()) atomic_store(&peer->failed, 1);
            }
            if (state == CLOSING) {
                if (atomic_load(&peer->failed) || (!available(&peer->transmit) && atomic_load(&peer->eof)) || shoop_millis() >= peer->close_at) {
                    lwip_close(peer->fd);
                    atomic_store_explicit(&peer->state, FREE, memory_order_release);
                } else if (!atomic_load(&peer->eof)) {
                    int count = lwip_recv(peer->fd, peer->receive.bytes, COPY_LIMIT, 0);
                    if (count == 0) atomic_store(&peer->eof, 1);
                    else if (count < 0 && !again()) atomic_store(&peer->failed, 1);
                }
                continue;
            }
            if (atomic_load(&peer->eof) || atomic_load(&peer->failed)) continue;
            size_t space = CAPACITY - available(&peer->receive);
            if (space) {
                uint32_t written = atomic_load(&peer->receive.written);
                int count = lwip_recv(peer->fd, peer->receive.bytes + written % CAPACITY, portion(written, space), 0);
                if (count > 0) atomic_store_explicit(&peer->receive.written, written + count, memory_order_release);
                else if (count == 0) atomic_store(&peer->eof, 1);
                else if (!again()) atomic_store(&peer->failed, 1);
            }
        }
        shoop_deploy_broker();
        vTaskDelay(1);
    }
}
int shoop_listen(uint16_t port) {
    unsigned slot = port == 15400 ? 0 : 1;
    int fd = lwip_socket(AF_INET, SOCK_STREAM, 0);
    if (fd < 0) return -1;
    struct sockaddr_in address = { .sin_family = AF_INET, .sin_port = lwip_htons(port) };
    if (lwip_bind(fd, (struct sockaddr *)&address, sizeof(address)) || lwip_listen(fd, 4) || nonblocking(fd)) {
        lwip_close(fd); return -1;
    }
    listeners[slot] = fd;
    if (slot == 1) configASSERT(xTaskCreate(broker, "socket-io", 4096, NULL, 4, NULL) == pdPASS);
    return slot;
}
int shoop_accept(int listener) {
    for (unsigned slot = 0; slot < SLOTS; ++slot) {
        struct connection *peer = &connections[slot];
        if (atomic_load_explicit(&peer->state, memory_order_acquire) == ACCEPTED && peer->listener == (unsigned)listener) {
            atomic_store(&peer->state, OPEN);
            return (peer->generation << 5) | slot;
        }
    }
    return -1;
}
static struct connection *lookup(int handle) {
    unsigned slot = (unsigned)handle & 31;
    if (handle < 0 || slot >= SLOTS) return NULL;
    struct connection *peer = &connections[slot];
    return atomic_load(&peer->state) == OPEN && peer->generation == (unsigned)handle >> 5 ? peer : NULL;
}
int shoop_read(int handle, unsigned char *output, size_t size) {
    struct connection *peer = lookup(handle);
    if (!peer) return -1;
    size_t count = available(&peer->receive);
    if (!count) return atomic_load(&peer->failed) ? -1 : atomic_load(&peer->eof) ? 0 : -2;
    uint32_t read = atomic_load(&peer->receive.read);
    count = portion(read, count < size ? count : size);
    memcpy(output, peer->receive.bytes + read % CAPACITY, count);
    atomic_store_explicit(&peer->receive.read, read + count, memory_order_release);
    return count;
}
int shoop_write(int handle, const unsigned char *input, size_t size) {
    struct connection *peer = lookup(handle);
    if (!peer || atomic_load(&peer->failed)) return -1;
    size_t space = CAPACITY - available(&peer->transmit);
    if (!space) return -2;
    uint32_t written = atomic_load(&peer->transmit.written);
    size_t count = portion(written, space < size ? space : size);
    memcpy(peer->transmit.bytes + written % CAPACITY, input, count);
    atomic_store_explicit(&peer->transmit.written, written + count, memory_order_release);
    return count;
}
void shoop_close(int handle) {
    struct connection *peer = lookup(handle);
    if (!peer) return;
    peer->close_at = shoop_millis() + 2000;
    atomic_store_explicit(&peer->state, CLOSING, memory_order_release);
}
unsigned shoop_socket_count(void) {
    unsigned count = 0;
    for (unsigned slot = 0; slot < SLOTS; ++slot) count += atomic_load(&connections[slot].state) != FREE;
    return count;
}
void shoop_network_stop(void) {
    atomic_store(&stopping, 1);
    uint64_t deadline = shoop_millis() + 10000;
    while (!atomic_load(&stopped) && shoop_millis() < deadline) vTaskDelay(1);
    configASSERT(atomic_load(&stopped));
    configASSERT(shoop_socket_count() == 0);
}
