#include <utility>
#define IMPLEMENT_MIDICHANNEL_H
#include "MidiChannel.h"
#include "MidiPortInterface.h"
#include "channel_mode_helpers.h"
#include <memory>
#include <optional>
#include <functional>
#include <chrono>
#include <thread>
template class MidiChannel<uint32_t, uint16_t>;
template class MidiChannel<uint32_t, uint32_t>;
template class MidiChannel<uint16_t, uint16_t>;
template class MidiChannel<uint16_t, uint32_t>;

using namespace std::chrono_literals;
using namespace logging;

template <typename TimeType, typename SizeType>
std::string MidiChannel<TimeType, SizeType>::log_module_name() const {
    return "Backend.MidiChannel";
}

template <typename TimeType, typename SizeType>
MidiChannel<TimeType, SizeType>::ExternalBufState::ExternalBufState()
    : n_events_total(0), n_events_processed(0), n_frames_total(0),
      n_frames_processed(0) {}

template <typename TimeType, typename SizeType>
size_t MidiChannel<TimeType, SizeType>::ExternalBufState::frames_left() const {
    return n_frames_total - n_frames_processed;
}
template <typename TimeType, typename SizeType>
size_t MidiChannel<TimeType, SizeType>::ExternalBufState::events_left() const {
    return n_events_total - n_events_processed;
}

template <typename TimeType, typename SizeType>
MidiChannel<TimeType, SizeType>::TrackedState::TrackedState(bool notes,
                                                            bool controls,
                                                            bool programs)
    : m_valid(false),
      state(std::make_shared<MidiStateTracker>(notes, controls, programs)),
      diff(std::make_shared<MidiStateDiffTracker>()) {}

template <typename TimeType, typename SizeType>
typename MidiChannel<TimeType, SizeType>::TrackedState &
MidiChannel<TimeType, SizeType>::TrackedState::operator=(
    TrackedState const &other) {
    // We want to track the same source state, yet copy
    // the cached state contents from the other tracker
    // so that the diffs may diverge.
    m_valid = other.m_valid;
    state->copy_relevant_state(*other.state);
    diff->set_diff(other.diff->get_diff());
    diff->reset(other.diff->a(), state, StateDiffTrackerAction::None);
    return *this;
}

template <typename TimeType, typename SizeType>
void
MidiChannel<TimeType, SizeType>::TrackedState::set_from(std::shared_ptr<MidiStateTracker> &t) {
    state->copy_relevant_state(*t);
    diff->reset(t, state, StateDiffTrackerAction::ClearDiff);
    m_valid = true;
}

template <typename TimeType, typename SizeType>
void
MidiChannel<TimeType, SizeType>::TrackedState::reset() { m_valid = false; }

template <typename TimeType, typename SizeType>
bool
MidiChannel<TimeType, SizeType>::TrackedState::valid() const { return m_valid; }

template <typename TimeType, typename SizeType>
void
MidiChannel<TimeType, SizeType>::TrackedState::set_valid(bool v) { m_valid = v; }

template <typename TimeType, typename SizeType>
void
MidiChannel<TimeType, SizeType>::TrackedState::resolve_to_output(
    std::function<void(size_t size, uint8_t *data)> send_cb) {
    if (m_valid) {
        diff->resolve_to_a(send_cb);
    }
}

template <typename TimeType, typename SizeType>
MidiChannel<TimeType, SizeType>::MidiChannel(size_t data_size, channel_mode_t mode,
            std::shared_ptr<profiling::Profiler> maybe_profiler)
    : WithCommandQueue<10, 1000, 1000>(),
      mp_playback_target_buffer(std::make_pair(ExternalBufState(), nullptr)),
      mp_recording_source_buffer(std::make_pair(ExternalBufState(), nullptr)),
      mp_storage(std::make_shared<Storage>(data_size)),
      mp_prerecord_storage(std::make_shared<Storage>(data_size)),
      mp_playback_cursor(nullptr), ma_mode(mode), ma_data_length(0),
      mp_output_midi_state(
          std::make_shared<MidiStateTracker>(true, true, true)),
      mp_input_midi_state(
          std::make_shared<MidiStateTracker>(false, true, true)),
      mp_track_start_state(false, true, true),
      mp_pre_playback_state(false, true, true),
      mp_track_prerecord_start_state(false, true, true),
      ma_n_events_triggered(0), ma_start_offset(0), ma_data_seq_nr(0),
      ma_pre_play_samples(0), mp_prev_pos_after(0), mp_prev_process_flags(0),
      ma_last_played_back_sample(0), ma_prerecord_data_length(0) {
    log_init();
    log_trace();
    mp_playback_cursor = mp_storage->create_cursor();
    if (maybe_profiler) {
        mp_profiling_item = maybe_profiler->maybe_get_profiling_item(
            "Process.Loops.MidiChannels");
    }
}

template <typename TimeType, typename SizeType>
MidiChannel<TimeType, SizeType>::~MidiChannel() { log<LogLevel::debug>("Destroyed"); }


template <typename TimeType, typename SizeType>
MidiChannel<TimeType, SizeType> &MidiChannel<TimeType, SizeType>::operator=(MidiChannel<TimeType, SizeType> const &other) {
    log_trace();

    mp_playback_target_buffer = other.mp_playback_target_buffer;
    mp_recording_source_buffer = other.mp_recording_source_buffer;
    mp_storage = other.mp_storage;
    mp_playback_cursor = other.mp_playback_cursor;
    ma_mode = other.ma_mode.load();
    ma_n_events_triggered = other.ma_n_events_triggered.load();
    mp_output_midi_state = other.mp_output_midi_state;
    mp_track_start_state = other.mp_track_start_state;
    mp_track_prerecord_start_state = other.mp_track_prerecord_start_state;
    ma_start_offset = other.ma_start_offset.load();
    ma_data_length = other.ma_data_length.load();
    mp_prev_process_flags = other.mp_prev_process_flags;
    *mp_prerecord_storage = *other.mp_prerecord_storage;
    ma_prerecord_data_length = other.ma_prerecord_data_length.load();
    *mp_input_midi_state = *other.mp_input_midi_state;
    mp_pre_playback_state = other.mp_pre_playback_state;
    mp_profiling_item = other.mp_profiling_item;
    data_changed();
    return *this;
}

template <typename TimeType, typename SizeType>
unsigned
MidiChannel<TimeType, SizeType>::get_data_seq_nr() const { return ma_data_seq_nr; }

template <typename TimeType, typename SizeType>
void
MidiChannel<TimeType, SizeType>::data_changed() {
    log_trace();
    ma_data_seq_nr++;
}

template <typename TimeType, typename SizeType>
size_t
MidiChannel<TimeType, SizeType>::get_length() const { return ma_data_length; }

template <typename TimeType, typename SizeType>
void
MidiChannel<TimeType, SizeType>::PROC_set_length_impl(Storage &storage, std::atomic<size_t> &storage_length,
                          size_t length) {
    if (storage_length != length) {
        storage.truncate(length);
        storage_length = length;
        data_changed();
    }
}

template <typename TimeType, typename SizeType>
void
MidiChannel<TimeType, SizeType>::PROC_set_length(size_t length) {
    PROC_set_length_impl(*mp_storage, ma_data_length, length);
}

template <typename TimeType, typename SizeType>
void
MidiChannel<TimeType, SizeType>::set_pre_play_samples(size_t samples) { ma_pre_play_samples = samples; }

template <typename TimeType, typename SizeType>
size_t
MidiChannel<TimeType, SizeType>::get_pre_play_samples() const { return ma_pre_play_samples; }

template <typename TimeType, typename SizeType>
void
MidiChannel<TimeType, SizeType>::PROC_process(loop_mode_t mode, std::optional<loop_mode_t> maybe_next_mode,
                  std::optional<size_t> maybe_next_mode_delay_cycles,
                  std::optional<size_t> maybe_next_mode_eta, size_t n_samples,
                  size_t pos_before, size_t pos_after, size_t length_before,
                  size_t length_after) {
    profiling::stopwatch(
        [&, this]() {
            log_trace();

            PROC_handle_command_queue();

            auto process_params = get_channel_process_params(
                mode, maybe_next_mode, maybe_next_mode_delay_cycles,
                maybe_next_mode_eta, pos_before, ma_start_offset, ma_mode);
            auto const &process_flags = process_params.process_flags;

            // We always need to process input messages to keep our input port
            // state up-to-date. This is done inside the recording process
            // function, but if we don't record, we need to do it here.
            bool processed_input_messages = false;

            // If we were playing back and anything other than forward playback
            // is happening (e.g. mode switch, position set, ...), send an All
            // Sound Off.
            bool playback_interrupted =
                (mp_prev_process_flags & ChannelPlayback) &&
                ((!process_flags & ChannelPlayback) ||
                 pos_before != mp_prev_pos_after);
            if (playback_interrupted) {
                log<LogLevel::debug>("Playback interrupted -> All Sound Off");
                PROC_send_all_sound_off();
            }

            if (!(process_flags & ChannelPreRecord) &&
                (mp_prev_process_flags & ChannelPreRecord)) {
                // Ending pre-record. If transitioning to recording,
                // make our pre-recorded buffers into our main buffers.
                // Otherwise, just discard them.
                if (process_flags & ChannelRecord) {
                    log<LogLevel::debug>(
                        "Pre-record end -> carry over to record");
                    mp_storage = mp_prerecord_storage;
                    ma_data_length = ma_start_offset =
                        ma_prerecord_data_length.load();
                    mp_track_start_state = mp_track_prerecord_start_state;
                    mp_track_prerecord_start_state.reset();
                } else {
                    log<LogLevel::debug>("Pre-record end -> discard");
                }
                mp_prerecord_storage =
                    std::make_shared<Storage>(mp_storage->bytes_capacity());
                ma_prerecord_data_length = 0;
            }

            if (process_flags & ChannelPlayback) {
                // If we are starting playback:
                //   - Reset the playback cursor
                //   - set up the state management.
                // By copying the state tracker, we now have a playback state
                // tracker that holds the diff between the actual MIDI channel
                // state and the state that we should have at this point in
                // playback. That diff will be updated as we process messages,
                // while keeping the original start state intact.
                // mp_played_anything_this_loop tracks whether we have played
                // back anything yet. Upon the first message played back, we can
                // restore the state to what it should be at that point.
                if (!(mp_prev_process_flags & ChannelPlayback)) {
                    mp_playback_cursor->reset();
                    mp_pre_playback_state = mp_track_start_state;
                }

                ma_last_played_back_sample = process_params.position;
                PROC_process_playback(process_params.position, length_before,
                                      n_samples, false);
            } else {
                ma_last_played_back_sample = -1;
            }
            if (process_flags & ChannelRecord) {
                PROC_process_record(*mp_storage, ma_data_length,
                                    mp_track_start_state,
                                    length_before + ma_start_offset, n_samples);
                processed_input_messages = true;
            } else if (process_flags & ChannelPreRecord) {
                if (!(mp_prev_process_flags & ChannelPreRecord)) {
                    log<LogLevel::debug>("Pre-record start");
                }
                PROC_process_record(*mp_prerecord_storage,
                                    ma_prerecord_data_length,
                                    mp_track_prerecord_start_state,
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
        },
        mp_profiling_item);
}

template <typename TimeType, typename SizeType>
void
MidiChannel<TimeType, SizeType>::PROC_finalize_process() {
    profiling::stopwatch([&, this]() { log_trace(); }, mp_profiling_item);
}

template <typename TimeType, typename SizeType>
void
MidiChannel<TimeType, SizeType>::PROC_process_input_messages(size_t n_samples) {
    log_trace();

    auto &recbuf = mp_recording_source_buffer.value();
    auto n = std::min(recbuf.first.frames_left(), n_samples);
    if (n == 0) {
        return;
    }

    size_t end = recbuf.first.n_frames_processed + n;
    for (size_t idx = recbuf.first.n_events_processed; idx < recbuf.first.n_events_total;
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
            mp_input_midi_state->process_msg(d);
            recbuf.first.n_events_processed++;
        }
    }
}

template <typename TimeType, typename SizeType>
void
MidiChannel<TimeType, SizeType>::PROC_process_record(Storage &storage,
                         std::atomic<size_t> &storage_data_length,
                         TrackedState &track_start_state, size_t record_from,
                         size_t n_samples) {
    log_trace();

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
    size_t record_end = recbuf.first.n_frames_processed + n_samples;
    for (size_t idx = recbuf.first.n_events_processed; idx < recbuf.first.n_events_total;
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
                    log<LogLevel::debug>("Caching port state for record");
                    track_start_state.set_from(mp_input_midi_state);
                }

                storage.append(record_from + (TimeType)t -
                                   recbuf.first.n_frames_processed,
                               (SizeType)s, d);
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

template <typename TimeType, typename SizeType>
void
MidiChannel<TimeType, SizeType>::clear(bool thread_safe) {
    log_trace();

    auto fn = [this]() {
        mp_storage->clear();
        mp_playback_cursor->reset();
        mp_output_midi_state->clear();
        mp_track_start_state.reset();
        mp_track_prerecord_start_state.reset();
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

template <typename TimeType, typename SizeType>
void
MidiChannel<TimeType, SizeType>::PROC_send_all_sound_off() {
    auto &buf = mp_playback_target_buffer.value();
    if (buf.first.frames_left() < 1) {
        throw_error<std::runtime_error>(
            "Attempting to play back out of bounds");
    }
    PROC_send_message_ref(*buf.second, all_sound_off_message_channel_0);
}

template <typename TimeType, typename SizeType>
void
MidiChannel<TimeType, SizeType>::PROC_send_message_ref(MidiWriteableBufferInterface &buf, MidiSortableMessageInterface const &event) {
    log_trace();

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

template <typename TimeType, typename SizeType>
void
MidiChannel<TimeType, SizeType>::PROC_send_message_value(MidiWriteableBufferInterface &buf, size_t time, size_t size, uint8_t *data) {
    log_trace();

    if (!buf.write_by_value_supported()) {
        throw_error<std::runtime_error>(
            "Midi write buffer does not support value write method");
    }
    buf.PROC_write_event_value(size, time, data);
    mp_output_midi_state->process_msg(data);
}

template <typename TimeType, typename SizeType>
void
MidiChannel<TimeType, SizeType>::PROC_process_playback(size_t our_pos, size_t our_length, size_t n_samples,
                           bool muted) {
    log_trace();

    if (!mp_playback_target_buffer.has_value()) {
        throw_error<std::runtime_error>("Playing without target buffer");
    }
    auto &buf = mp_playback_target_buffer.value();
    if (buf.first.frames_left() < n_samples) {
        throw_error<std::runtime_error>(
            "Attempting to play back out of bounds");
    }
    auto _pos = (int)our_pos;

    // Playback any events.
    size_t end = buf.first.n_frames_processed + n_samples;
    // Start by skipping messages up to our time point.
    // Skipped messages should still be fed into the state tracker to ensure we
    // can restore correct control state when playing our first message.
    mp_playback_cursor->find_time_forward(
        std::max(0, _pos), [&](typename Storage::Elem *e) {
            if (mp_pre_playback_state.valid()) {
                mp_pre_playback_state.state->process_msg(e->data());
            }
        });

    int valid_from =
        std::max(_pos, (int)ma_start_offset - (int)ma_pre_play_samples);
    int valid_to = _pos + (int)n_samples;
    while (mp_playback_cursor->valid()) {
        auto *event = mp_playback_cursor->get();
        if ((int)event->storage_time >= valid_to) {
            // Future event
            break;
        }
        if (!muted && (int)event->storage_time >= valid_from) {
            // See if we need to restore any cached MIDI channel state by
            // sending additional messages.
            auto proc_time =
                (int)event->storage_time - _pos + buf.first.n_frames_processed;

            if (mp_pre_playback_state.valid()) {
                log<LogLevel::debug>(
                    "Restoring port state for playback @ sample {}",
                    event->storage_time);
                mp_pre_playback_state.resolve_to_output(
                    [this, &buf, &proc_time](size_t size, uint8_t *data) {
                        log<LogLevel::debug>("  - Restore msg: {} {} {}",
                                             data[0], data[1], data[2]);
                        PROC_send_message_value(*buf.second, proc_time, size,
                                                data);
                    });
                mp_pre_playback_state.set_valid(false);
            }

            event->proc_time = proc_time;
            PROC_send_message_ref(*buf.second, *event);
            ma_n_events_triggered++;
        }
        if (mp_pre_playback_state.valid()) {
            mp_pre_playback_state.state->process_msg(event->get_data());
        }
        buf.first.n_events_processed++;
        mp_playback_cursor->next();
    }
}


template <typename TimeType, typename SizeType>
std::optional<size_t>
MidiChannel<TimeType, SizeType>::
PROC_get_next_poi(loop_mode_t mode, std::optional<loop_mode_t> maybe_next_mode,
                  std::optional<size_t> maybe_next_mode_delay_cycles,
                  std::optional<size_t> maybe_next_mode_eta, size_t length,
                  size_t position) const {
    std::optional<size_t> rval = std::nullopt;
    auto merge_poi = [&rval](size_t poi) {
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

template <typename TimeType, typename SizeType>
void
MidiChannel<TimeType, SizeType>::PROC_set_playback_buffer(MidiWriteableBufferInterface *buffer,
                              size_t n_frames) {
    log_trace();
    mp_playback_target_buffer = std::make_pair(ExternalBufState(), buffer);
    mp_playback_target_buffer.value().first.n_frames_total = n_frames;
}

template <typename TimeType, typename SizeType>
void
MidiChannel<TimeType, SizeType>::PROC_set_recording_buffer(MidiReadableBufferInterface *buffer,
                               size_t n_frames) {
    log_trace();
    mp_recording_source_buffer =
        std::make_pair(ExternalBufState(), buffer);
    mp_recording_source_buffer.value().first.n_frames_total = n_frames;
    mp_recording_source_buffer.value().first.n_events_total =
        buffer->PROC_get_n_events();
}

template <typename TimeType, typename SizeType>
std::vector<typename MidiChannel<TimeType, SizeType>::Message> 
MidiChannel<TimeType, SizeType>::retrieve_contents(bool thread_safe) {
    log_trace();

    auto s = std::make_shared<Storage>(mp_storage->bytes_capacity());
    auto fn = [this, &s]() { mp_storage->copy(*s); };
    if (thread_safe) {
        exec_process_thread_command(fn);
    } else {
        fn();
    }

    std::vector<Message> r;
    s->for_each_msg([&r](TimeType time, SizeType size, uint8_t *data) {
        r.push_back(Message(time, size, std::vector<uint8_t>(size)));
        memcpy((void *)r.back().data.data(), (void *)data, size);
    });
    return r;
}

template <typename TimeType, typename SizeType>
void
MidiChannel<TimeType, SizeType>::set_contents(std::vector<Message> contents, size_t length_samples,
                  bool thread_safe) {
    log_trace();

    auto s = std::make_shared<Storage>(mp_storage->bytes_capacity());
    for (auto const &elem : contents) {
        s->append(elem.time, elem.size, elem.data.data());
    }

    auto fn = [this, &s, length_samples]() {
        mp_storage = s;
        mp_playback_cursor = mp_storage->create_cursor();
        PROC_set_length(length_samples);
        data_changed();
    };

    if (thread_safe) {
        exec_process_thread_command(fn);
    } else {
        fn();
    }
}

template <typename TimeType, typename SizeType>
void
MidiChannel<TimeType, SizeType>::PROC_handle_poi(loop_mode_t mode, size_t length, size_t position) {}

template <typename TimeType, typename SizeType>
void
MidiChannel<TimeType, SizeType>::set_mode(channel_mode_t mode) { ma_mode = mode; }

template <typename TimeType, typename SizeType>
channel_mode_t
MidiChannel<TimeType, SizeType>::get_mode() const { return ma_mode; }

template <typename TimeType, typename SizeType>
size_t
MidiChannel<TimeType, SizeType>::get_n_notes_active() const {
    return mp_output_midi_state->n_notes_active();
}

template <typename TimeType, typename SizeType>
size_t
MidiChannel<TimeType, SizeType>::get_n_events_triggered() {
    auto rval = ma_n_events_triggered.load();
    ma_n_events_triggered = 0;
    return rval;
}

template <typename TimeType, typename SizeType>
void
MidiChannel<TimeType, SizeType>::set_start_offset(int offset) { ma_start_offset = offset; }

template <typename TimeType, typename SizeType>
int
MidiChannel<TimeType, SizeType>::get_start_offset() const { return ma_start_offset; }

template <typename TimeType, typename SizeType>
std::optional<size_t>
MidiChannel<TimeType, SizeType>::get_played_back_sample() const { return std::nullopt; }