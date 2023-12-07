#include "ConnectedLoop.h"
#include "ConnectedChannel.h"
#include "AudioMidiLoop.h"
#include "BackendSession.h"

void ConnectedLoop::PROC_prepare_process(uint32_t n_frames) {
    for (auto &chan : mp_audio_channels) {
        chan->PROC_prepare_process_audio(n_frames);
    }
    for (auto &chan : mp_midi_channels) {
        chan->PROC_prepare_process_midi(n_frames);
    }
}

void ConnectedLoop::PROC_finalize_process() {
    for (auto &chan : mp_audio_channels) {
        chan->PROC_finalize_process_audio();
    }
    for (auto &chan : mp_midi_channels) {
        chan->PROC_finalize_process_midi();
    }
}

void ConnectedLoop::delete_audio_channel_idx(uint32_t idx, bool thread_safe) {
    get_backend().queue_process_thread_command([this, idx]() {
        auto chaninfo = mp_audio_channels.at(idx);
        delete_midi_channel(chaninfo, false);
    });
}

void ConnectedLoop::delete_midi_channel_idx(uint32_t idx, bool thread_safe) {
    get_backend().queue_process_thread_command([this, idx]() {
        auto chaninfo = mp_midi_channels.at(idx);
        delete_midi_channel(chaninfo, false);
    });
}

void ConnectedLoop::delete_audio_channel(std::shared_ptr<ConnectedChannel> chan, bool thread_safe) {
    auto fn = [this, chan]() {
        auto r = std::find_if(mp_audio_channels.begin(), mp_audio_channels.end(), [chan](auto const& e) { return chan->channel.get() == e->channel.get(); });
        if (r == mp_audio_channels.end()) {
            throw std::runtime_error("Attempting to delete non-existent audio channel.");
        }
        loop->delete_audio_channel(chan->channel, false);
        mp_audio_channels.erase(r);
    };
    if (thread_safe) {
        get_backend().queue_process_thread_command(fn);
    } else {
        fn();
    }
}

void ConnectedLoop::delete_midi_channel(std::shared_ptr<ConnectedChannel> chan, bool thread_safe) {
    auto fn = [this, chan]() {
        auto r = std::find_if(mp_midi_channels.begin(), mp_midi_channels.end(), [chan](auto const& e) { return chan->channel.get() == e->channel.get(); });
        if (r == mp_midi_channels.end()) {
            throw std::runtime_error("Attempting to delete non-existent midi channel.");
        }
        loop->delete_midi_channel(chan->channel, false);
        mp_midi_channels.erase(r);
    };
    if (thread_safe) {
        get_backend().queue_process_thread_command(fn);
    } else {
        fn();
    }
}

void ConnectedLoop::delete_all_channels(bool thread_safe) {
    auto fn = [this]() {
        for (auto &chan : mp_audio_channels) {
            loop->delete_audio_channel(chan->channel, false);
        }
        mp_audio_channels.clear();
        for (auto &chan : mp_midi_channels) {
            loop->delete_midi_channel(chan->channel, false);
        }
        mp_midi_channels.clear();
    };
    if (thread_safe) {
        get_backend().queue_process_thread_command(fn);
    } else {
        fn();
    }
}

BackendSession &ConnectedLoop::get_backend() {
    auto b = backend.lock();
    if(!b) {
        throw std::runtime_error("Back-end no longer exists");
    }
    return *b;
}