#include <memory>
#include <vector>
#include "limits.h"

class AudioMidiLoop;
class ConnectedChannel;
class Backend;

struct ConnectedLoop : public std::enable_shared_from_this<ConnectedLoop> {

    const std::shared_ptr<AudioMidiLoop> loop;
    std::vector<std::shared_ptr<ConnectedChannel>> mp_audio_channels;
    std::vector<std::shared_ptr<ConnectedChannel>>  mp_midi_channels;
    std::weak_ptr<Backend> backend;

    ConnectedLoop(std::shared_ptr<Backend> backend,
             std::shared_ptr<AudioMidiLoop> loop) :
        loop(loop),
        backend(backend) {
        mp_audio_channels.reserve(limits::default_max_audio_channels);
        mp_midi_channels.reserve(limits::default_max_midi_channels);
    }

    void delete_audio_channel_idx(size_t idx, bool thread_safe=true);
    void delete_midi_channel_idx(size_t idx, bool thread_safe=true);
    void delete_audio_channel(std::shared_ptr<ConnectedChannel> chan, bool thread_safe=true);
    void delete_midi_channel(std::shared_ptr<ConnectedChannel> chan, bool thread_safe=true);
    void delete_all_channels(bool thread_safe=true);
    void PROC_prepare_process(size_t n_frames);
    void PROC_finalize_process();
    Backend &get_backend();
};