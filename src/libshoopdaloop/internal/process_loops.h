#pragma once
#include <LoopInterface.h>
#include <vector>
#include <memory>
#include <optional>
#include <functional>
#include <stdexcept>
#include <iostream>
#include <set>

constexpr uint32_t g_max_0_procs = 10;

template<typename Loop, typename SharedLoopsBegin, typename SharedLoopsEnd>
void process_loops(SharedLoopsBegin loops_begin,
                   SharedLoopsEnd loops_end,
                   uint32_t n_samples,
                   std::function<LoopInterface*(Loop&)> loop_getter = [](Loop &l) { return static_cast<LoopInterface*>(&l); },
                   uint32_t n_recursive_0_procs=0) {

    if (n_recursive_0_procs > g_max_0_procs) {
        throw std::runtime_error("Stuck in recursive 0-processing loop");
    }

    uint32_t process_until = n_samples;

    // Gather up all POIs.
    for(auto l = loops_begin; l != loops_end; l++) {
        if(*l) {
            uint32_t poi = loop_getter(*l->get())->PROC_get_next_poi().value_or(n_samples);
            process_until = std::min(process_until, poi);
        }
    }

    // Process until the first POI, then handle POIs and triggers.
    for(auto l = loops_begin; l != loops_end; l++) {
        if(*l) {
            loop_getter(*l->get())->PROC_process(process_until);
        }
    }
    for(auto l = loops_begin; l != loops_end; l++) {
        if(*l) {
            loop_getter(*l->get())->PROC_handle_poi();
        }
    }
    for(auto l = loops_begin; l != loops_end; l++) {
        if(*l) {
            loop_getter(*l->get())->PROC_handle_sync();
        }
    }

    // If we didn't process the whole thing, keep going.
    if(process_until < n_samples) {
        process_loops(loops_begin, loops_end, n_samples - process_until, loop_getter, n_recursive_0_procs + 1);
    }
}