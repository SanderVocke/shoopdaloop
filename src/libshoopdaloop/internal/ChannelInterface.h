#pragma once
#include <optional>
#include <memory>
#include <stdio.h>
#include <utility>
#include "types.h"
#include "PortInterface.h"

// A generic channel is a class which can run as a dependent on an actual loop. It does
// not manage its own mode transitions, position or length, but instead is tightly
// controlled by its parent loop.
class ChannelInterface {
public:
    // Get the next point of interest in ticks.
    // A point of interest is the first point until when the loop can be processed
    // without updating its state.
    // Returning a nullopt_t indicates that the loop may be processed indefinitely.
    virtual std::optional<uint32_t> PROC_get_next_poi(shoop_loop_mode_t mode,
                                               std::optional<shoop_loop_mode_t> maybe_next_mode,
                                               std::optional<uint32_t> maybe_next_mode_delay_cycles,
                                               std::optional<uint32_t> maybe_next_mode_eta,
                                               uint32_t length,
                                               uint32_t position) const = 0;

    // Handle the current point of interest, leading to any internal state change
    // necessary. If the loop is not currently exactly at a point of interest,
    // nothing happens.
    virtual void PROC_handle_poi(shoop_loop_mode_t mode,
                            uint32_t length,
                            uint32_t position) = 0;

    // Process the channel according to the loop state.
    virtual void PROC_process(
        shoop_loop_mode_t mode,
        std::optional<shoop_loop_mode_t> maybe_next_mode,
        std::optional<uint32_t> maybe_next_mode_delay_cycles,
        std::optional<uint32_t> maybe_next_mode_eta,
        uint32_t n_samples,
        uint32_t pos_before,
        uint32_t pos_after,
        uint32_t length_before,
        uint32_t length_after
    ) = 0;

    // Finalize processing. For some channels, the processing step defers some
    // actions for later. Channels which support deferred processing (state changes first,
    // memory copies later) can use this to perform memory copies.
    virtual void PROC_finalize_process() = 0;

    // Set/get the channel mode
    virtual void set_mode(shoop_channel_mode_t mode) = 0;
    virtual shoop_channel_mode_t get_mode() const = 0;

    // Set/get the channel length
    virtual void PROC_set_length(uint32_t length) = 0;
    virtual uint32_t get_length() const = 0;

    // Set/get the playback start offset.
    // This offset is the sample # which starts playing at the moment
    // a transition to playback or loop around happens.
    virtual void set_start_offset(int offset) = 0;
    virtual int get_start_offset() const = 0;

    // Set/get the amount of pre-play samples.
    // For context: when a change to recording mode is coming, the channel is
    // silently already recording. This "pre-recording" can be used e.g. to
    // play a fill or riff leading into the loop. When pre-play samples is >0,
    // these pre-recorded samples will also be pre-played.
    virtual void set_pre_play_samples(uint32_t samples) = 0;
    virtual uint32_t get_pre_play_samples() const = 0;

    // Get the last played back sample, if any.
    virtual std::optional<uint32_t> get_played_back_sample() const = 0;

    // Get a sequence number which increments whenever the content, of this channel changes.
    // Can be used for e.g. "dirty" detection.
    virtual unsigned get_data_seq_nr() const = 0;

    virtual void adopt_ringbuffer_contents(
        std::shared_ptr<PortInterface> from_port,
        std::optional<unsigned> reverse_start_offset,
        bool thread_safe
    ) = 0;

    ChannelInterface() = default;
    virtual ~ChannelInterface() {}
};