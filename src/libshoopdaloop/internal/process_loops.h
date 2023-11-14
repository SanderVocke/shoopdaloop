#pragma once
#include <LoopInterface.h>
#include <vector>
#include <memory>
#include <optional>
#include <functional>
#include <stdexcept>
#include <iostream>

constexpr uint32_t g_max_0_procs = 10;

template<typename Loop>
void process_loops(std::vector<std::shared_ptr<Loop>> const& loops_access,
                   uint32_t n_samples,
                   std::function<LoopInterface*(Loop&)> loop_getter = [](Loop &l) { return static_cast<LoopInterface*>(&l); },
                   uint32_t n_recursive_0_procs=0) {

    if (n_recursive_0_procs > g_max_0_procs) {
        throw std::runtime_error("Stuck in recursive 0-processing loop");
    }

    uint32_t process_until = n_samples;

    // Gather up all POIs.
    for(uint32_t i=0; i<loops_access.size(); i++) {
        if(loops_access[i]) {
            uint32_t poi = loop_getter(*loops_access[i])->PROC_get_next_poi().value_or(n_samples);
            process_until = std::min(process_until, poi);
        }
    }

    // Process until the first POI, then handle POIs and triggers.
    for(uint32_t i=0; i<loops_access.size(); i++) {
        if(loops_access[i]) {
            loop_getter(*loops_access[i].get())->PROC_process(process_until);
        }
    }
    for(uint32_t i=0; i<loops_access.size(); i++) {
        if(loops_access[i]) {
            loop_getter(*loops_access[i].get())->PROC_handle_poi();
        }
    }
    for(uint32_t i=0; i<loops_access.size(); i++) {
        if(loops_access[i]) {
            loop_getter(*loops_access[i].get())->PROC_handle_sync();
        }
    }

    // If we didn't process the whole thing, keep going.
    if(process_until < n_samples) {
        process_loops(loops_access, n_samples - process_until, loop_getter, n_recursive_0_procs + 1);
    }
}