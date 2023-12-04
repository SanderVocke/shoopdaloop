#include "AudioMidiDrivers.h"
#include "LoggingBackend.h"
#include "DummyAudioMidiDriver.h"
#include "JackAudioMidiDriver.h"
#include "shoop_globals.h"

AudioMidiDriver &create_audio_midi_driver(shoop_audio_driver_type_t type) {
    AudioMidiDriver* rval;
    bool tried_dummy = false;

    auto init_dummy = [&]() {
      tried_dummy = true;
      rval = static_cast<AudioMidiDriver*>(
        new shoop_types::_DummyAudioMidiDriver()
      );
    };

    try {
      switch (type) {
#ifdef SHOOP_HAVE_BACKEND_JACK
      case Jack:
          log<log_debug>("Creating JACK audio driver instance.");
          rval = static_cast<AudioMidiDriver*>(
            new JackAudioMidiDriver()
          );
          break;
      case JackTest:
          log<log_debug>("Creating JACK test audio driver instance.");
          rval = static_cast<AudioMidiDriver*>(
            new JackTestAudioMidiDriver()
          );
          break;
#else
      case Jack:
      case JackTest:
          log<log_warning>("JACK audio driver requested but not supported.");
#endif
      case Dummy:
          log<log_debug>("Creating dummy audio driver instance.");
          init_dummy();
          break;
      default:
          log<log_warning>("Unknown or unsupported audio driver type requested. Falling back to dummy driver.");
          init_dummy();
      }
    } catch (std::exception &e) {
      log<log_error>("Failed to initialize audio driver.");
      log<log_info>("Failure log_info: " + std::string(e.what()));

      if (!tried_dummy) {
        log<log_info>("Attempting fallback to dummy audio system.");
        init_dummy();
      }
    } catch (...) {
      log<log_error>(
          "Failed to initialize audio system: unknown exception");
      
      if (!tried_dummy) {
        log<log_info>("Attempting fallback to dummy audio system.");
        init_dummy();
      }
    }

    if (!rval) { throw std::runtime_error("Failed to create audio driver."); }
    return *rval;
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