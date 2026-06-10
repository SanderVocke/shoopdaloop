#include "backend_rust/src/jack_api_cxx.rs.h"
#include "internal/jack/JackApiCxxTrampolines.h"
#include "internal/jack/JackAudioMidiDriver.h"
#include "internal/LoggingBackend.h"
#include <string>

namespace backend_rust {

int jackapi_process_cb(uintptr_t driver_ptr, uint32_t nframes) {
    auto *driver = reinterpret_cast<JackAudioMidiDriver *>(driver_ptr);
    return driver ? driver->PROC_process_cb_inst(nframes) : -1;
}

int jackapi_xrun_cb(uintptr_t driver_ptr) {
    auto *driver = reinterpret_cast<JackAudioMidiDriver *>(driver_ptr);
    return driver ? driver->PROC_xrun_cb_inst() : -1;
}

void jackapi_port_update_cb(uintptr_t driver_ptr) {
    auto *driver = reinterpret_cast<JackAudioMidiDriver *>(driver_ptr);
    if (driver) {
        driver->PROC_update_ports_cb_inst();
    }
}

void jackapi_error_log(rust::Str msg) {
    logging::log<"Backend.JackAudioMidiDriver", log_level_error>(
        std::nullopt,
        std::nullopt,
        std::string("JACK error: ") + std::string(msg)
    );
}

void jackapi_info_log(rust::Str msg) {
    logging::log<"Backend.JackAudioMidiDriver", log_level_info>(
        std::nullopt,
        std::nullopt,
        std::string("JACK info: ") + std::string(msg)
    );
}

}
