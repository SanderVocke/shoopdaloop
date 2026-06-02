#include <cstdint>

#if defined(_MSC_VER)
#define SHOOP_WEAK
#else
#define SHOOP_WEAK __attribute__((weak))
#endif

namespace backend_rust {

// Weak fallback definitions for standalone backend_rust crate tests/builds, where
// the full C++ backend registry object file is not linked. Full backend builds link
// BridgeObject.cpp, whose strong definitions override these weak no-op fallbacks.
SHOOP_WEAK bool bridge_upgrade_for_rust(uint64_t, uint32_t) {
    return false;
}

SHOOP_WEAK void bridge_release_strong_for_rust(uint64_t, uint32_t) {}

}
