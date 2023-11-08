#pragma once

namespace shoop_types {

enum class ProcessWhen {
    BeforeFXChains, // Process before FX chains have processed.
    AfterFXChains   // Process only after FX chains have processed.
};

}