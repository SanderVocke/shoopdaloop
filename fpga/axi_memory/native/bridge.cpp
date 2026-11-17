#include "memory_system.h"
#include <cmath>
#include <cstdint>
#include <deque>
#include <map>
#include <memory>
#include <stdexcept>
#include <string>

struct Completion {
    uint64_t id, address, submitted, completed;
    uint32_t write;
};

struct BusSnapshot {
    uint64_t ticks, read_cycles, write_cycles, read_commands, write_commands, command_cycles;
};

class Bridge {
 public:
    std::unique_ptr<dramsim3::MemorySystem> memory;
    std::map<std::pair<uint64_t, bool>, std::deque<Completion>> pending;
    std::deque<Completion> completed;
    uint64_t ticks = 0, phase = 0, next_id = 1, outstanding = 0, tck_fs;

    Bridge(const char* config, const char* output) {
        memory.reset(new dramsim3::MemorySystem(config, output,
            [this](uint64_t address) { finish(address, false); },
            [this](uint64_t address) { finish(address, true); }));
        tck_fs = std::llround(memory->GetTCK()*1000000.0);
        if (tck_fs == 0) throw std::runtime_error("invalid DRAM clock");
    }

    void finish(uint64_t address, bool write) {
        auto it = pending.find({address, write});
        if (it == pending.end() || it->second.empty()) throw std::runtime_error("unmatched DRAM callback");
        auto event = it->second.front();
        it->second.pop_front();
        if (it->second.empty()) pending.erase(it);
        event.completed = ticks;
        completed.push_back(event);
        --outstanding;
    }

    void tick() { ++ticks; memory->ClockTick(); }
};

static thread_local std::string last_error;

extern "C" {
const char* shoop_dram_error() { return last_error.c_str(); }

void* shoop_dram_create(const char* config, const char* output) {
    try { return new Bridge(config, output); }
    catch (const std::exception& error) { last_error = error.what(); return nullptr; }
}

void shoop_dram_destroy(Bridge* bridge) { delete bridge; }
uint64_t shoop_dram_tck_fs(Bridge* bridge) { return bridge->tck_fs; }
uint32_t shoop_dram_native_bytes(Bridge* bridge) {
    return bridge->memory->GetBusBits()/8*bridge->memory->GetBurstLength();
}
uint64_t shoop_dram_ticks(Bridge* bridge) { return bridge->ticks; }
uint32_t shoop_dram_abi_version() { return 2; }
void shoop_dram_bus_snapshot(Bridge* bridge, BusSnapshot* result) {
    auto counters = bridge->memory->GetBusCounters();
    *result = {bridge->ticks, counters.read_cycles, counters.write_cycles,
               counters.read_commands, counters.write_commands, counters.command_cycles};
}

uint64_t shoop_dram_submit(Bridge* bridge, uint64_t address, uint32_t write) {
    if (bridge->outstanding >= static_cast<uint64_t>(bridge->memory->GetQueueSize())
        || !bridge->memory->WillAcceptTransaction(address, write != 0)) return 0;
    auto id = bridge->next_id++;
    bridge->pending[{address, write != 0}].push_back({id, address, bridge->ticks, 0, write});
    ++bridge->outstanding;
    bridge->memory->AddTransaction(address, write != 0);
    return id;
}

void shoop_dram_advance(Bridge* bridge, uint64_t fabric_hz) {
    bridge->phase += 1000000000000000ULL;
    auto period = bridge->tck_fs*fabric_hz;
    while (bridge->phase >= period) { bridge->phase -= period; bridge->tick(); }
}

uint32_t shoop_dram_poll(Bridge* bridge, Completion* result) {
    if (bridge->completed.empty()) return 0;
    *result = bridge->completed.front();
    bridge->completed.pop_front();
    return 1;
}

uint32_t shoop_dram_idle(Bridge* bridge) {
    return bridge->outstanding == 0 && bridge->memory->IsIdle();
}

uint32_t shoop_dram_drain(Bridge* bridge, uint64_t max_ticks) {
    for (uint64_t i = 0; i < max_ticks; ++i) {
        if (shoop_dram_idle(bridge)) return 1;
        bridge->tick();
    }
    return shoop_dram_idle(bridge);
}

void shoop_dram_print_stats(Bridge* bridge) { bridge->memory->PrintStats(); }
}
