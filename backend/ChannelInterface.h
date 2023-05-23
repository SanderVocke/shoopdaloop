#pragma once
#include <optional>
#include <memory>
#include <stdio.h>
#include <utility>
#include "types.h"

// A generic channel is a class which can run as a dependent on an actual loop. It does
// not manage its own mode transitions, position or length, but instead is tightly
// controlled by its parent loop.
class ChannelInterface {
public:
    // Get the next point of interest in ticks.
    // A point of interest is the first point until when the loop can be processed
    // without updating its state.
    // Returning a nullopt_t indicates that the loop may be processed indefinitely.
    virtual std::optional<size_t> PROC_get_next_poi(loop_mode_t mode,
                                               std::optional<std::pair<loop_mode_t, size_t>> maybe_next_mode,
                                               size_t length,
                                               size_t position) const = 0;

    // Handle the current point of interest, leading to any internal state change
    // necessary. If the loop is not currently exactly at a point of interest,
    // nothing happens.
    virtual void PROC_handle_poi(loop_mode_t mode,
                            size_t length,
                            size_t position) = 0;

    // Process the channel according to the loop state.
    virtual void PROC_process(
        loop_mode_t mode,
        std::optional<std::pair<loop_mode_t, size_t>> maybe_next_mode,
        size_t n_samples,
        size_t pos_before,
        size_t pos_after,
        size_t length_before,
        size_t length_after
    ) = 0;

    // Finalize processing. For some channels, the processing step defers some
    // actions for later. Channels which support deferred processing (state changes first,
    // memory copies later) can use this to perform memory copies.
    virtual void PROC_finalize_process() = 0;

    // Set/get the channel mode
    virtual void set_mode(channel_mode_t mode) = 0;
    virtual channel_mode_t get_mode() const = 0;

    // Set/get the channel length
    virtual void PROC_set_length(size_t length) = 0;
    virtual size_t get_length() const = 0;

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
    virtual void set_pre_play_samples(size_t samples) = 0;
    virtual size_t get_pre_play_samples() const = 0;

    // Get a sequence number which increments whenever the content, of this channel changes.
    // Can be used for e.g. "dirty" detection.
    virtual unsigned get_data_seq_nr() const = 0;

    ChannelInterface() = default;
    virtual ~ChannelInterface() {}
};