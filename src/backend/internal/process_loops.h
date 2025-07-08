#pragma once
#include <LoopInterface.h>
#include <vector>
#include <memory>
#include <optional>
#include <functional>
#include <stdexcept>
#include <iostream>
#include <set>

#ifdef _WIN32
#undef min
#undef max
#endif

constexpr uint32_t g_max_0_procs = 10;

template<typename LoopIterator, typename SharedLoopsBegin, typename SharedLoopsEnd>
void process_loops(SharedLoopsBegin loops_begin,
                   SharedLoopsEnd loops_end,
                   uint32_t n_samples,
                   std::function<LoopInterface*(LoopIterator &)> loop_getter = [](LoopIterator &l) -> LoopInterface* { return shoop_static_pointer_cast<LoopInterface>(*l).get(); },
                   uint32_t n_recursive_0_procs=0) {
    if (n_recursive_0_procs > g_max_0_procs) {
        throw std::runtime_error("Stuck in recursive 0-processing loop");
    }

    uint32_t process_until = n_samples;

    // Gather up all POIs.
    for(auto l = loops_begin; l != loops_end; l++) {
        if(LoopInterface* _l = loop_getter(l)) {
            uint32_t poi = _l->PROC_get_next_poi().value_or(n_samples);
            process_until = std::min(process_until, poi);
        }
    }

    // Process until the first POI, then handle POIs and triggers.
    for(auto l = loops_begin; l != loops_end; l++) {
        if(LoopInterface* _l = loop_getter(l)) {
            _l->PROC_process(process_until);
        }
    }
    for(auto l = loops_begin; l != loops_end; l++) {
        if(LoopInterface* _l = loop_getter(l)) {
            _l->PROC_handle_poi();
        }
    }
    for(auto l = loops_begin; l != loops_end; l++) {
        if(LoopInterface* _l = loop_getter(l)) {
            _l->PROC_handle_sync();
        }
    }

    // If we didn't process the whole thing, keep going.
    if(process_until < n_samples) {
        process_loops(loops_begin, loops_end, n_samples - process_until, loop_getter,
                      (n_samples == 0 ?
                       n_recursive_0_procs + 1 :
                       n_recursive_0_procs));
    }
}