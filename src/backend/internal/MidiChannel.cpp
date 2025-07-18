#include <utility>
#include "LoggingBackend.h"
#include "MidiChannel.h"
#include "MidiPort.h"
#include "MidiStateDiffTracker.h"
#include "MidiStateTracker.h"
#include "MidiStorage.h"
#include "channel_mode_helpers.h"
#include "types.h"
#include <memory>
#include <optional>
#include <functional>
#include <chrono>
#include <thread>
#include <fmt/format.h>

#ifdef _WIN32
#undef min
#undef max
#endif

using namespace std::chrono_literals;
using namespace logging;

MidiChannel::ExternalBufState::ExternalBufState()
    : n_events_total(0), n_events_processed(0), n_frames_total(0),
      n_frames_processed(0) {}

uint32_t MidiChannel::ExternalBufState::frames_left() const {
    return n_frames_total - n_frames_processed;
}
uint32_t MidiChannel::ExternalBufState::events_left() const {
    return n_events_total - n_events_processed;
}

MidiChannel::MidiChannel(uint32_t data_size, shoop_channel_mode_t mode)
    : WithCommandQueue(50),
      mp_playback_target_buffer(std::make_pair(ExternalBufState(), nullptr)),
      mp_recording_source_buffer(std::make_pair(ExternalBufState(), nullptr)),
      mp_storage(shoop_make_shared<Storage>(data_size)),
      mp_prerecord_storage(shoop_make_shared<Storage>(data_size)),
      mp_playback_cursor(nullptr), ma_mode(mode), ma_data_length(0),
      mp_output_midi_state(
          shoop_make_shared<MidiStateTracker>(true, true, true)),
      mp_input_midi_state(
          shoop_make_shared<MidiStateTracker>(true, true, true)),
      mp_recording_start_state_tracker(shoop_make_shared<TrackedRelativeMidiState>(true, true, true)),
      mp_track_state_until_first_msg_playback(shoop_make_shared<TrackedRelativeMidiState>(true, true, true)),
      mp_temp_prerecording_start_state_tracker(shoop_make_shared<TrackedRelativeMidiState>(true, true, true)),
      ma_n_events_triggered(0), ma_start_offset(0), ma_data_seq_nr(0),
      ma_pre_play_samples(0), mp_prev_pos_after(0), mp_prev_process_flags(0),
      ma_last_played_back_sample(0), ma_prerecord_data_length(0) {
    mp_playback_cursor = mp_storage->create_cursor();
}

MidiChannel::~MidiChannel() { log<log_level_debug>("Destroyed"); }


MidiChannel &MidiChannel::operator=(MidiChannel const &other) {

    mp_playback_target_buffer = other.mp_playback_target_buffer;
    mp_recording_source_buffer = other.mp_recording_source_buffer;
    mp_storage = other.mp_storage;
    mp_playback_cursor = other.mp_playback_cursor;
    ma_mode = other.ma_mode.load();
    ma_n_events_triggered = other.ma_n_events_triggered.load();
    mp_output_midi_state = other.mp_output_midi_state;
    if (mp_recording_start_state_tracker && other.mp_recording_start_state_tracker) {
        *mp_recording_start_state_tracker = *other.mp_recording_start_state_tracker;
    }
    if (mp_temp_prerecording_start_state_tracker && other.mp_temp_prerecording_start_state_tracker) {
        *mp_temp_prerecording_start_state_tracker = *other.mp_temp_prerecording_start_state_tracker;
    }
    if (mp_track_state_until_first_msg_playback && other.mp_track_state_until_first_msg_playback) {
        *mp_track_state_until_first_msg_playback = *other.mp_track_state_until_first_msg_playback;
    }
    ma_start_offset = other.ma_start_offset.load();
    ma_data_length = other.ma_data_length.load();
    mp_prev_process_flags = other.mp_prev_process_flags;
    *mp_prerecord_storage = *other.mp_prerecord_storage;
    ma_prerecord_data_length = other.ma_prerecord_data_length.load();
    *mp_input_midi_state = *other.mp_input_midi_state;
    mp_profiling_item = other.mp_profiling_item;
    data_changed();
    return *this;
}

unsigned
MidiChannel::get_data_seq_nr() const { return ma_data_seq_nr; }

void
MidiChannel::data_changed() {
    ma_data_seq_nr++;
}

uint32_t
MidiChannel::get_length() const { return ma_data_length; }

void
MidiChannel::PROC_set_length_impl(Storage &storage, std::atomic<uint32_t> &storage_length,
                          uint32_t length) {
    if (storage_length != length) {
        storage.truncate(length, Storage::TruncateSide::TruncateHead);
        storage_length = length;
        data_changed();
    }
}

void
MidiChannel::PROC_set_length(uint32_t length) {
    log<log_level_debug_trace>("length -> {}", length);
    PROC_set_length_impl(*mp_storage, ma_data_length, length);
}

void
MidiChannel::set_pre_play_samples(uint32_t samples) {
    log<log_level_debug>("n preplay -> {}", samples);
    ma_pre_play_samples = samples;
}

uint32_t
MidiChannel::get_pre_play_samples() const { return ma_pre_play_samples; }

void
MidiChannel::PROC_process(shoop_loop_mode_t mode, std::optional<shoop_loop_mode_t> maybe_next_mode,
                  std::optional<uint32_t> maybe_next_mode_delay_cycles,
                  std::optional<uint32_t> maybe_next_mode_eta, uint32_t n_samples,
                  uint32_t pos_before, uint32_t pos_after, uint32_t length_before,
                  uint32_t length_after) {
    PROC_handle_command_queue();

    auto process_params = get_channel_process_params(
        mode, maybe_next_mode, maybe_next_mode_delay_cycles,
        maybe_next_mode_eta, pos_before, ma_start_offset, ma_mode);
    auto &process_flags = process_params.process_flags;

    // Corner case: a just-created channel may immediately
    // go into pre-record mode before being properly added to the
    // processing graph (TODO: better fix).
    // Here, we solve it by only going ahead when buffers have been
    // assigned.
    if ((!mp_recording_source_buffer.has_value()) ||
        (mp_recording_source_buffer.value().second == nullptr)) {
        process_flags &= (~ChannelPreRecord & ~ChannelRecord & ~ChannelReplace);
    }
    if (!mp_playback_target_buffer.has_value() ||
        (mp_playback_target_buffer.value().second == nullptr)) {
        process_flags &= (~ChannelPlayback);
    }

    // We always need to process input messages to keep our input port
    // state up-to-date. This is done inside the recording process
    // function, but if we don't record, we need to do it here.
    bool processed_input_messages = false;

    // If we were playing back and anything other than forward playback
    // is happening (e.g. mode switch, position set, ...), send an All
    // Sound Off.
    bool playback_interrupted =
        (mp_prev_process_flags & ChannelPlayback) &&
        ((!(process_flags & ChannelPlayback)) ||
            pos_before != mp_prev_pos_after);
    if (playback_interrupted && n_samples > 0) {
        auto time = mp_playback_target_buffer->first.n_frames_processed;
        log<log_level_debug>("Playback interrupted -> All Sound Off @ {}", time);
        PROC_send_all_sound_off(time);
    }

    if (!(process_flags & ChannelPreRecord) &&
        (mp_prev_process_flags & ChannelPreRecord)) {
        // Ending pre-record. If transitioning to recording,
        // make our pre-recorded buffers into our main buffers.
        // Otherwise, just discard them.
        if (process_flags & ChannelRecord) {
            auto n_msgs = mp_prerecord_storage->n_events();
            log<log_level_debug>(
                "Pre-record end -> carry over {} pre-recorded msgs to record", n_msgs);
            mp_storage = mp_prerecord_storage;
            mp_playback_cursor = mp_storage->create_cursor();
            ma_data_length = ma_start_offset =
                ma_prerecord_data_length.load();
            *mp_recording_start_state_tracker = *mp_temp_prerecording_start_state_tracker;
            mp_temp_prerecording_start_state_tracker->reset();
        } else {
            log<log_level_debug>("Pre-record end -> discard");
        }
        mp_prerecord_storage =
            shoop_make_shared<Storage>(mp_storage->bytes_capacity());
        ma_prerecord_data_length = 0;
    }

    if (process_flags & ChannelPlayback) {
        // If we are (re-)starting playback:
        //   - Reset the playback cursor
        //   - set up the state management.
        // By copying the state tracker, we now have a playback state
        // tracker that holds the diff between the actual MIDI channel
        // state and the state that we should have at this point in
        // playback. That diff will be updated as we process messages,
        // while keeping the original start state intact.
        // Upon the first message played back, we can
        // restore the state to what it should be at that point.
        if (!(mp_prev_process_flags & ChannelPlayback) || (ma_last_played_back_sample > process_params.position)) {
            log<log_level_debug_trace>("Playback start, reset cursor, prev flags {}, last {}, pos {}, start state {} (valid {})",
                mp_prev_process_flags,
                ma_last_played_back_sample.load(),
                process_params.position,
                fmt::ptr(mp_recording_start_state_tracker->state.get()),
                mp_recording_start_state_tracker->valid()
            );
            mp_playback_cursor->reset();
            *mp_track_state_until_first_msg_playback = *mp_recording_start_state_tracker;
        }

        PROC_process_playback(process_params.position, length_before,
                                n_samples, false);
    } else if (ma_last_played_back_sample.load() >= 0) {
        log<log_level_debug_trace>("Playback ended, clearing last played sample");
        ma_last_played_back_sample = -1;
    }
    if (process_flags & ChannelRecord) {
        PROC_process_record(*mp_storage, ma_data_length,
                            *mp_recording_start_state_tracker,
                            length_before + ma_start_offset, n_samples);
        processed_input_messages = true;
    } else if (process_flags & ChannelPreRecord) {
        if (!(mp_prev_process_flags & ChannelPreRecord)) {
            log<log_level_debug>("Pre-record start");
        }
        PROC_process_record(*mp_prerecord_storage,
                            ma_prerecord_data_length,
                            *mp_temp_prerecording_start_state_tracker,
                            ma_prerecord_data_length, n_samples);
        processed_input_messages = true;
    }

    mp_prev_pos_after = pos_after;
    mp_prev_process_flags = process_flags;

    if (!processed_input_messages) {
        PROC_process_input_messages(n_samples);
    }

    // Update recording/playback buffers.
    if (mp_recording_source_buffer.has_value()) {
        mp_recording_source_buffer->first.n_frames_processed += n_samples;
    }
    if (mp_playback_target_buffer.has_value()) {
        mp_playback_target_buffer->first.n_frames_processed += n_samples;
    }
}

void
MidiChannel::PROC_finalize_process() {}

void
MidiChannel::PROC_process_input_messages(uint32_t n_samples) {

    auto &recbuf = mp_recording_source_buffer.value();
    auto n = std::min(recbuf.first.frames_left(), n_samples);
    if (n == 0) {
        return;
    }

    uint32_t end = recbuf.first.n_frames_processed + n;
    for (uint32_t idx = recbuf.first.n_events_processed; idx < recbuf.first.n_events_total;
         idx++) {
        uint32_t t;
        uint32_t s;
        const uint8_t *d;
        recbuf.second->PROC_get_event_reference(idx).get(s, t, d);
        if (t >= end) {
            // Will handle in a future process iteration
            break;
        } else {
            // Regardless of whether we record or not, we need to keep the input
            // port state up-to-date.
            log<log_level_debug_trace>("Basic processing of msg {} of {}", idx, recbuf.first.n_events_total);
            mp_input_midi_state->process_msg(d);
            recbuf.first.n_events_processed++;
        }
    }
}

void
MidiChannel::PROC_process_record(Storage &storage,
                         std::atomic<uint32_t> &storage_data_length,
                         TrackedRelativeMidiState &track_start_state, uint32_t record_from,
                         uint32_t n_samples) {
    log<log_level_debug_trace>("record {} frames", n_samples);

    if (!mp_recording_source_buffer.has_value()) {
        throw_error<std::runtime_error>("Recording without source buffer");
    }
    auto &recbuf = mp_recording_source_buffer.value();
    if (recbuf.first.frames_left() < n_samples) {
        throw_error<std::runtime_error>("Attempting to record out of bounds");
    }
    bool changed = false;

    // Truncate buffer if necessary
    PROC_set_length_impl(storage, storage_data_length, record_from);

    // Record any incoming events
    uint32_t record_end = recbuf.first.n_frames_processed + n_samples;
    for (uint32_t idx = recbuf.first.n_events_processed; idx < recbuf.first.n_events_total;
         idx++) {
        uint32_t t;
        uint32_t s;
        const uint8_t *d;
        recbuf.second->PROC_get_event_reference(idx).get(s, t, d);
        if (t >= record_end) {
            // Will handle in a future process iteration
            break;
        } else {
            if (t >=
                recbuf.first.n_frames_processed) { // Don't store any message that
                                                   // came before our process window
                // If here, we are about to record a message.
                // If it is the first recorded message, this is also the moment
                // to cache the MIDI state on the input (such as hold pedal,
                // other CCs, pitch wheel, etc.) so we can restore it later.
                if (storage.n_events() == 0) {
                    log<log_level_debug>("cache port state {} -> {} for record", fmt::ptr(mp_input_midi_state.get()), fmt::ptr(track_start_state.state.get()));
                    track_start_state.start_tracking_from(mp_input_midi_state);
                }
                log<log_level_debug_trace>("record msg: {} {} {}",
                    (s > 0) ? (int)d[0] : -1,
                    (s > 1) ? (int)d[1] : -1,
                    (s > 2) ? (int)d[2] : -1
                );
                storage.append(record_from + (uint32_t)t -
                                   recbuf.first.n_frames_processed,
                               (uint16_t)s, d);
                changed = true;
            }

            // Regardless of whether we record or not, we need to keep the input
            // port state up-to-date.
            mp_input_midi_state->process_msg(d);

            recbuf.first.n_events_processed++;
        }
    }

    PROC_set_length_impl(storage, storage_data_length,
                         storage_data_length + n_samples);
    if (changed) {
        data_changed();
    }
}

void
MidiChannel::clear(bool thread_safe) {
    auto fn = [this]() {
        log<log_level_debug>("clear");
        mp_storage->clear();
        mp_playback_cursor->reset();
        mp_output_midi_state->clear();
        mp_recording_start_state_tracker->reset();
        mp_temp_prerecording_start_state_tracker->reset();
        ma_n_events_triggered = 0;
        PROC_set_length(0);
        ma_start_offset = 0;
        data_changed();
    };
    if (thread_safe) {
        exec_process_thread_command(fn);
    } else {
        fn();
    }
}

void
MidiChannel::PROC_send_all_sound_off(unsigned frame) {
    auto &buf = mp_playback_target_buffer.value();
    static const uint8_t all_sound_off_data[] = {0xB0, 120, 0};
    if (buf.first.frames_left() < 1) {
        throw_error<std::runtime_error>(
            "Attempting to play back out of bounds");
    }
    PROC_send_message_value(*buf.second,
        frame, 3, (uint8_t*) all_sound_off_data);
}

void
MidiChannel::PROC_reset_midi_state_tracking() {
    log<log_level_debug>("Reset state tracking");
    mp_output_midi_state = shoop_make_shared<MidiStateTracker>(true, true, true);
    mp_input_midi_state = shoop_make_shared<MidiStateTracker>(true, true, true);
}

void
MidiChannel::PROC_send_message_ref(MidiWriteableBufferInterface &buf, MidiSortableMessageInterface const &event) {

    if (buf.write_by_reference_supported()) {
        buf.PROC_write_event_reference(event);
        mp_output_midi_state->process_msg(event.get_data());
    } else if (buf.write_by_value_supported()) {
        buf.PROC_write_event_value(event.get_size(), event.get_time(),
                                   event.get_data());
        mp_output_midi_state->process_msg(event.get_data());
    } else {
        throw_error<std::runtime_error>(
            "Midi write buffer does not support any write methods");
    }
}

void
MidiChannel::PROC_send_message_value(MidiWriteableBufferInterface &buf, uint32_t time, uint32_t size, uint8_t *data) {

    if (!buf.write_by_value_supported()) {
        throw_error<std::runtime_error>(
            "Midi write buffer does not support value write method");
    }
    buf.PROC_write_event_value(size, time, data);
    mp_output_midi_state->process_msg(data);
}

void
MidiChannel::PROC_process_playback(uint32_t our_pos, uint32_t our_length, uint32_t n_samples,
                           bool muted) {
    if (!mp_playback_target_buffer.has_value()) {
        throw_error<std::runtime_error>("Playing without target buffer");
    }
    auto &buf = mp_playback_target_buffer.value();
    if (buf.first.frames_left() < n_samples) {
        throw_error<std::runtime_error>(
            "Attempting to play back out of bounds");
    }
    auto _pos = (int)our_pos;

    log<log_level_debug_trace>("playback {} frames, start {}, buf {}, last {}, {} msgs total",
        n_samples, our_pos, buf.first.n_frames_processed, ma_last_played_back_sample.load(), mp_storage->n_events());

    // Playback any events.
    uint32_t end = buf.first.n_frames_processed + n_samples;
    // Start by skipping messages up to our time point.
    // Skipped messages should still be fed into the state tracker to ensure we
    // can restore correct control state when playing our first message.
    mp_playback_cursor->find_time_forward(
        std::max(0, _pos), [&](typename Storage::Elem *e) {
            if (mp_track_state_until_first_msg_playback->valid()) {
                log<log_level_debug_trace>("process pre-playback message");
                mp_track_state_until_first_msg_playback->state->process_msg(e->data());
            } else {
                log<log_level_debug_trace>("ignore pre-playback message: tracker not enabled");
            }
            ma_last_played_back_sample = e->storage_time;
        });

    if (mp_playback_cursor->valid()) {
        log<log_level_debug_trace>("playback: first upcoming msg is @ {}", mp_playback_cursor->get()->storage_time);
    } else {
        log<log_level_debug_trace>("playback: no upcoming msgs");
    }

    int valid_from =
        std::max(_pos, (int)ma_start_offset - (int)ma_pre_play_samples);
    int valid_to = _pos + (int)n_samples;
    while (mp_playback_cursor->valid()) {
        auto *event = mp_playback_cursor->get();

        // If there is an upcoming event to play back in the future, and we have a pending
        // pre-playback state, we need to resolve it now.
        if (mp_track_state_until_first_msg_playback->valid() &&    // <-- there is a state to restore
            (int)event->storage_time >= valid_from &&              // <-- the upcoming recorded event will be "playable"
            !muted &&                                              // <-- we are not muted
            (our_pos + n_samples) > valid_from)                    // <-- we are in the playback window
        {
            log<log_level_debug_trace>("Resolve pre-playback state");
            mp_track_state_until_first_msg_playback->resolve_to_output(
                [this, &buf](uint32_t size, uint8_t *data) {
                    // Play state resolving msgs ASAP (at current buffer pos)
                    auto time = mp_playback_target_buffer->first.n_frames_processed;
                    log<log_level_debug_trace>("  - Restore msg @ {}: {} {} {}",
                                        time,
                                        data[0], data[1], data[2]);
                    PROC_send_message_value(*buf.second, time, size, data);
                });
            mp_track_state_until_first_msg_playback->set_valid(false);
        }

        // Now, we move on to playing back the recorded content.
        if ((int)event->storage_time >= valid_to) {
            // Future event
            break;
        }
        if ((int)event->storage_time >= valid_from) {
            if (muted) {
                log<log_level_debug_trace>("playback: skip msg @ {} (muted)", event->storage_time);
            } else {
                // See if we need to restore any cached MIDI channel state by
                // sending additional messages.
                auto proc_time =
                    (int)event->storage_time - _pos + buf.first.n_frames_processed;

                log<log_level_debug_trace>("playback: play msg @ {}", event->storage_time);
                event->proc_time = proc_time;
                PROC_send_message_ref(*buf.second, *event);
                ma_last_played_back_sample = event->storage_time;
                ma_n_events_triggered++;
            }
        }
        if (mp_track_state_until_first_msg_playback->valid()) {
            log<log_level_debug>("playback: skip msg but apply to state @ {}", event->storage_time);
            mp_track_state_until_first_msg_playback->state->process_msg(event->get_data());
        }
        buf.first.n_events_processed++;
        mp_playback_cursor->next();
    }
    if (mp_playback_cursor->valid()) {
        log<log_level_debug_trace>("playback: done. first upcoming msg is @ {}", mp_playback_cursor->get()->storage_time);
    } else {
        log<log_level_debug_trace>("playback: done, reached end.");
    }
    ma_last_played_back_sample = (int)(our_pos + n_samples) - 1;
}


std::optional<uint32_t>
MidiChannel::
PROC_get_next_poi(shoop_loop_mode_t mode, std::optional<shoop_loop_mode_t> maybe_next_mode,
                  std::optional<uint32_t> maybe_next_mode_delay_cycles,
                  std::optional<uint32_t> maybe_next_mode_eta, uint32_t length,
                  uint32_t position) const {
    std::optional<uint32_t> rval = std::nullopt;
    auto merge_poi = [&rval](uint32_t poi) {
        rval = rval.has_value() ? std::min(rval.value(), poi) : poi;
    };

    auto process_params = get_channel_process_params(
        mode, maybe_next_mode, maybe_next_mode_delay_cycles,
        maybe_next_mode_eta, position, ma_start_offset, ma_mode);

    if (ma_mode) {
        if (process_params.process_flags & ChannelPlayback) {
            merge_poi(mp_playback_target_buffer.value().first.n_frames_total -
                      mp_playback_target_buffer.value().first.n_frames_processed);
        }
        if (process_params.process_flags & (ChannelRecord | ChannelPreRecord)) {
            merge_poi(mp_recording_source_buffer.value().first.n_frames_total -
                      mp_recording_source_buffer.value().first.n_frames_processed);
        }
    }
    return rval;
}

void
MidiChannel::PROC_set_playback_buffer(MidiWriteableBufferInterface *buffer,
                              uint32_t n_frames) {
    mp_playback_target_buffer = std::make_pair(ExternalBufState(), buffer);
    mp_playback_target_buffer.value().first.n_frames_total = n_frames;
}

void
MidiChannel::PROC_set_recording_buffer(MidiReadableBufferInterface *buffer,
                               uint32_t n_frames) {
    if (!buffer) return;
    mp_recording_source_buffer =
        std::make_pair(ExternalBufState(), buffer);
    mp_recording_source_buffer.value().first.n_frames_total = n_frames;
    mp_recording_source_buffer.value().first.n_events_total =
        buffer->PROC_get_n_events();
}

typename MidiChannel::Contents
MidiChannel::retrieve_contents(bool thread_safe) {
    MidiStateTracker state(true, true, true);
    auto s = shoop_make_shared<Storage>(mp_storage->bytes_capacity());
    auto fn = [this, &s, &state]() {
        log<log_level_debug_trace>("retrieving contents");
        mp_storage->copy(*s);
        state.copy_relevant_state(*mp_recording_start_state_tracker->state);
    };
    if (thread_safe) {
        exec_process_thread_command(fn);
    } else {
        fn();
    }

    Contents rval;
    s->for_each_msg([&rval](uint32_t time, uint16_t size, uint8_t *data) {
        rval.recorded_msgs.push_back(Message(time, size, std::vector<uint8_t>(size)));
        memcpy((void *)rval.recorded_msgs.back().data.data(), (void *)data, size);
    });
    rval.starting_state_msg_datas = state.state_as_messages();
    return rval;
}

void
MidiChannel::set_contents(Contents contents, uint32_t length_samples,
                  bool thread_safe) {
    size_t n_state_msgs = contents.starting_state_msg_datas.size();

    shoop_shared_ptr<MidiStateTracker> new_start_state = shoop_make_shared<MidiStateTracker>(true, true, true);
    for (auto const &data : contents.starting_state_msg_datas) {
        new_start_state->process_msg(data.data());
    }

    // Calculate the needed storage size
    size_t min_storage_size = 1;
    for (auto const &elem : contents.recorded_msgs) {
        min_storage_size += MidiStorageElem::total_size_of (elem.data.size());
    }
    size_t new_storage_size = std::max((size_t)mp_storage->bytes_capacity(), min_storage_size);
    auto s = shoop_make_shared<Storage>(new_storage_size);

    for (auto const &elem : contents.recorded_msgs) {
        s->append(elem.time, elem.size, elem.data.data());
    }
    log<log_level_debug>("Loading data ({} messages + {} state messages in storage {}).", s->n_events(), n_state_msgs, fmt::ptr(s.get()));

    auto fn = [this, s, length_samples, new_start_state]() {
        log<log_level_debug_trace>("Applying loaded data (storage @ {}).", fmt::ptr(s.get()));
        mp_storage = s;
        mp_playback_cursor = mp_storage->create_cursor();
        mp_recording_start_state_tracker->start_tracking_from_with_state(mp_input_midi_state, new_start_state);
        PROC_set_length(length_samples);
        data_changed();
    };

    if (thread_safe) {
        exec_process_thread_command(fn);
    } else {
        fn();
    }
}

void
MidiChannel::PROC_handle_poi(shoop_loop_mode_t mode, uint32_t length, uint32_t position) {}

void
MidiChannel::set_mode(shoop_channel_mode_t mode) { ma_mode = mode; }

shoop_channel_mode_t
MidiChannel::get_mode() const { return ma_mode; }

uint32_t
MidiChannel::get_n_notes_active() const {
    return mp_output_midi_state->n_notes_active();
}

uint32_t
MidiChannel::get_n_events_triggered() {
    auto rval = ma_n_events_triggered.load();
    ma_n_events_triggered = 0;
    return rval;
}

void
MidiChannel::set_start_offset(int offset) {
    log<log_level_debug>("start offset -> {}", offset);
    ma_start_offset = offset;
}

int
MidiChannel::get_start_offset() const { return ma_start_offset; }

std::optional<uint32_t>
MidiChannel::get_played_back_sample() const {
    auto v = ma_last_played_back_sample.load();
    if (v >= 0) {
        return v;
    }
    return std::nullopt;
}

void
MidiChannel::adopt_ringbuffer_contents(shoop_shared_ptr<PortInterface> from_port,
    std::optional<unsigned> reverse_start_offset,
    std::optional<unsigned> keep_samples_before_start_offset,
    bool thread_safe) {

    if (reverse_start_offset.has_value()) {
        log<log_level_debug>("queue adopt ringbuffer @ reverse offset {}", reverse_start_offset.value());
    } else {
        log<log_level_debug>("queue adopt ringbuffer @ begin");
    }
    auto midiport = shoop_dynamic_pointer_cast<MidiPort>(from_port);
    if (!midiport) {
        log<log_level_error>("Cannot adopt MIDI ringbuffer from non-MIDI port");
        return;
    }

    auto fn = [midiport, reverse_start_offset, keep_samples_before_start_offset, this]() {
        shoop_shared_ptr<MidiStateTracker> maybe_start_tracking_state = nullptr;
        if (auto const& state = midiport->maybe_ringbuffer_tail_state_tracker()) {
            // What was the ringbuffer tail state should now become the recorded messages
            // start state of this channel.
            log<log_level_debug_trace>("adopting ringbuffer state tracker");
            maybe_start_tracking_state = shoop_make_shared<MidiStateTracker>(true, true, true);
            maybe_start_tracking_state->copy_relevant_state(*state);
        }

        midiport->PROC_snapshot_ringbuffer_into(*mp_storage);
        auto buflen = midiport->get_ringbuffer_n_samples();
        if(reverse_start_offset.has_value()) {
            set_start_offset((int)buflen - (int)reverse_start_offset.value());
        }
        if(keep_samples_before_start_offset.has_value()) {
            auto so = ma_start_offset.load();
            mp_storage->truncate(
                so - std::min((int)so, (int)keep_samples_before_start_offset.value()),
                MidiStorage::TruncateSide::TruncateTail,
                [this, maybe_start_tracking_state](uint32_t time, uint16_t size, const uint8_t* data) {
                    if (maybe_start_tracking_state) {
                        maybe_start_tracking_state->process_msg(data);
                    }
                });
        }

        auto n = mp_storage->n_events();

        log<log_level_debug_trace>("adopted midi with reverse so {}, ringbuffer len {}, keep {} - yielded {} messages, so {}",
             reverse_start_offset.value_or(99999),
             midiport->get_ringbuffer_n_samples(),
             keep_samples_before_start_offset.value_or(99999),
             n,
             get_start_offset());

        if (maybe_start_tracking_state) {
            mp_recording_start_state_tracker->
                start_tracking_from_with_state(mp_input_midi_state, maybe_start_tracking_state);
        }

        data_changed();
    };

    if (thread_safe) {
        queue_process_thread_command(fn);
    } else {
        fn();
    }
}