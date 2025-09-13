#include "LV2.h"
#include "ProcessProfiling.h"
#include "types.h"
#include <lilv_wrappers.h>

#ifdef _WIN32
#include "Windows.h"
#include <iostream>
#endif

using namespace logging;
namespace {
    std::atomic<bool> g_initialized = false;
}

LV2::LV2() : m_world(nullptr) {}

LV2::~LV2() { 
    if (m_world) {
        log<log_level_debug>("Freeing lilv world.");
        lilv_world_free(m_world);
    }
}

template <typename TimeType, typename SizeType>
shoop_shared_ptr<CarlaLV2ProcessingChain<TimeType, SizeType>>
LV2::create_carla_chain(shoop_fx_chain_type_t type, uint32_t sample_rate, uint32_t buffer_size,
                        std::string title, shoop_shared_ptr<typename AudioPort<shoop_types::audio_sample_t>::UsedBufferPool> maybe_buffer_pool) {
    if (!m_world) {
        if (!::g_initialized) {
        #ifdef _WIN32
            // Ensure we have default LV2 path set up
            char buf[32767];
            GetEnvironmentVariable("LV2_PATH", buf, sizeof(buf));
            std::string newval = std::string(buf) + ";C:\\Program Files\\Common Files\\LV2";
            SetEnvironmentVariable("LV2_PATH", newval.c_str());
            GetEnvironmentVariable("LV2_PATH", buf, sizeof(buf));
            log<log_level_debug>("LV2_PATH: {}", buf);
        #endif
            log<log_level_debug>("Initializing lilv.");
            initialize_lilv(1);
            ::g_initialized = true;
        }

        m_world = lilv_world_new();
        log<log_level_debug>("Lilv: world load all");
        lilv_world_load_all(m_world);
    }

    log<log_level_debug>("Create Carla chain.");
    return shoop_make_shared<CarlaLV2ProcessingChain<TimeType, SizeType>>(
        m_world, type, sample_rate, buffer_size, title, maybe_buffer_pool);
}

template shoop_shared_ptr<CarlaLV2ProcessingChain<uint32_t, uint16_t>>
LV2::create_carla_chain(
    shoop_fx_chain_type_t type, uint32_t sample_rate, uint32_t buffer_size, std::string title, shoop_shared_ptr<typename AudioPort<shoop_types::audio_sample_t>::UsedBufferPool> maybe_buffer_pool);
template shoop_shared_ptr<CarlaLV2ProcessingChain<uint32_t, uint32_t>>
LV2::create_carla_chain(
    shoop_fx_chain_type_t type, uint32_t sample_rate, uint32_t buffer_size, std::string title, shoop_shared_ptr<typename AudioPort<shoop_types::audio_sample_t>::UsedBufferPool> maybe_buffer_pool);
template shoop_shared_ptr<CarlaLV2ProcessingChain<uint16_t, uint16_t>>
LV2::create_carla_chain(
    shoop_fx_chain_type_t type, uint32_t sample_rate, uint32_t buffer_size, std::string title, shoop_shared_ptr<typename AudioPort<shoop_types::audio_sample_t>::UsedBufferPool> maybe_buffer_pool);
template shoop_shared_ptr<CarlaLV2ProcessingChain<uint16_t, uint32_t>>
LV2::create_carla_chain(
    shoop_fx_chain_type_t type, uint32_t sample_rate, uint32_t buffer_size, std::string title, shoop_shared_ptr<typename AudioPort<shoop_types::audio_sample_t>::UsedBufferPool> maybe_buffer_pool);