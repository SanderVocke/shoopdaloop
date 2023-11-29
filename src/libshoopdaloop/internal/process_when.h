#pragma once

namespace shoop_types {

// items are in chronological order
enum class ProcessWhen {
    BeforeLoops,    // Process before all loop/channel-related processing.
    BeforeFXChains, // Process before FX chains have processed.
    AfterFXChains,  // Process only after FX chains have processed.
    AfterLoops      // Process after all loop/channel-related processing.
};

}