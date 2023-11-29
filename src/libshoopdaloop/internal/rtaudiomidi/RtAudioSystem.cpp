#include "RtAudioSystem.h"
#include "LoggingBackend.h"
#include "MidiPortInterface.h"
#include "PortInterface.h"
#include <map>
#include <memory>

template<typename API>
GenericRtAudioSystem<API>::GenericRtAudioSystem(
        std::optional<std::string> input_device_name,
        std::optional<std::string> output_device_name,
        size_t input_channels,
        size_t output_channels,
        size_t sample_rate,
        size_t buffer_size,
        size_t n_buffers,
        std::function<void(uint32_t)> process_cb
    )
{
    std::map<std::string, int> devices;
    {
        RtAudio prober;
        for(int i=0; i<proper.getDeviceCount(); i++) {
            device_names_to_nrs[proper.getDeviceInfo(i).name] = i;
        }
    }
    int bufsize = (int) buffer_size;

    if (devices.find(input_device_name) == devices.end()) {
        throw_error("No such device: " + input_device_name);
    }
    if (devices.find(output_device_name) == devices.end()) {
        throw_error("No such device: " + output_device_name);
    }

    try {
        m_api = new API(
            devices.at(output_device_name),
            (int) output_channels,
            devices.at(input_device_name),
            (int) input_channels,
            RTAUDIO_FLOAT32,
            (int) sample_rate,
            &bufsize,
            (int)n_buffers
        );
    } catch (RtError &e) {
        throw_error("Failed to open device(s): {}", e.getMessage());
    } catch (std::exception &e) {
        throw_error("Failed to open device(s): {}", e.what());
    } catch (...) {
        throw_error("Failed to open device(s): unknown error");
    }
}

template class GenericRtAudioSystem<RtAudio>;