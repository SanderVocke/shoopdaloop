#include "AudioMidiDrivers.h"
#include "LoggingBackend.h"
#include "DummyAudioMidiDriver.h"
#include "shoop_globals.h"
#include <memory>

#ifdef SHOOP_HAVE_BACKEND_JACK
#include "JackAudioMidiDriver.h"
#endif

using namespace logging;

shoop_shared_ptr<AudioMidiDriver> create_audio_midi_driver(shoop_audio_driver_type_t type, void (*maybe_process_callback)()) {
    shoop_shared_ptr<AudioMidiDriver> rval;
    bool tried_dummy = false;

    auto init_dummy = [&]() {
      tried_dummy = true;
      rval = shoop_static_pointer_cast<AudioMidiDriver>(shoop_make_shared<shoop_types::_DummyAudioMidiDriver>(maybe_process_callback));
    };

    try {
      switch (type) {
#ifdef SHOOP_HAVE_BACKEND_JACK
      case Jack:
          log<"Backend.AudioMidiDrivers", log_level_debug>(std::nullopt, std::nullopt, "Creating JACK audio driver instance.");
          rval = shoop_static_pointer_cast<AudioMidiDriver>(shoop_make_shared<JackAudioMidiDriver>(maybe_process_callback));
          break;
      case JackTest:
          log<"Backend.AudioMidiDrivers", log_level_debug>(std::nullopt, std::nullopt, "Creating JACK test audio driver instance.");
          rval = shoop_static_pointer_cast<AudioMidiDriver>(shoop_make_shared<JackTestAudioMidiDriver>(maybe_process_callback));
          break;
#else
      case Jack:
      case JackTest:
          log<"Backend.AudioMidiDrivers", log_level_warning>(std::nullopt, std::nullopt, "JACK audio driver requested but not supported.");
#endif
      case Dummy:
          log<"Backend.AudioMidiDrivers", log_level_debug>(std::nullopt, std::nullopt, "Creating dummy audio driver instance.");
          init_dummy();
          break;
      default:
          log<"Backend.AudioMidiDrivers", log_level_warning>(std::nullopt, std::nullopt, "Unknown or unsupported audio driver type requested. Falling back to dummy driver.");
          init_dummy();
      }
    } catch (std::exception &e) {
      log<"Backend.AudioMidiDrivers", log_level_error>(std::nullopt, std::nullopt, "Failed to initialize audio driver.");
      log<"Backend.AudioMidiDrivers", log_level_info>(std::nullopt, std::nullopt, "Failure log_level_info: " + std::string(e.what()));

      if (!tried_dummy) {
        log<"Backend.AudioMidiDrivers", log_level_info>(std::nullopt, std::nullopt, "Attempting fallback to dummy audio system.");
        init_dummy();
      }
    } catch (...) {
      log<"Backend.AudioMidiDrivers", log_level_error>(std::nullopt, std::nullopt, 
          "Failed to initialize audio system: unknown exception");
      
      if (!tried_dummy) {
        log<"Backend.AudioMidiDrivers", log_level_info>(std::nullopt, std::nullopt, "Attempting fallback to dummy audio system.");
        init_dummy();
      }
    }

    if (!rval) { throw std::runtime_error("Failed to create audio driver."); }
    return rval;
}


bool audio_midi_driver_type_supported(shoop_audio_driver_type_t type) {
    switch(type) {
        case Dummy:
            return true;
            break;
        case Jack:
        case JackTest:
#ifdef SHOOP_HAVE_BACKEND_JACK
            return true;
#else
            return false;
#endif
            break;
        default:
            return false;
    }
}