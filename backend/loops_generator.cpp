#include "Halide.h"
#include "shoopdaloop_backend.h"

namespace {

using namespace Halide;
using namespace Halide::BoundaryConditions;

class Loops : public Halide::Generator<Loops> {
public:
    // Quantities:
    // Ns = samples per process loop
    // Np = number of input/output ports
    // Nl = number of loopers (they may share ports)
    // Ll = number of samples stored in a loop
    // NLat = number of samples in the latency buffer

    // Live input samples. Ns samples by Np ports
    Input<Buffer<float, 2>> samples_in{"samples_in"};

    // Latency storage buffer. NLat samples by Np ports
    Input<Buffer<float, 2>> latencybuf_in{"latencybuf_in"};
    // Current position in the latency buffer.
    Input<int32_t> latencybuf_writepos_in{"latencybuf_writepos_in"};
    // Latency for each port, in samples. Np ports
    Input<Buffer<int32_t, 1>> port_recording_latencies_in{"port_recording_latencies_in"};

    // Nl states, positions, lengths (one for each looper)
    Input<Buffer<int8_t, 1>> states_in{"states_in"};
    Input<Buffer<int32_t, 1>> positions_in{"positions_in"};
    Input<Buffer<int32_t, 1>> loop_lengths_in{"loop_lengths_in"};

    // Nl x 2
    // next_states holds the next 2 states planned for a loop.
    // next_state_countdowns holds a counter that determines how many
    // triggers it takes to move to the next state.
    // -1 means "never transition"
    Input<Buffer<int8_t, 2>> next_states_in{"next_states_in"};
    Input<Buffer<int8_t, 2>> next_state_countdowns_in{"next_state_countdowns_in"};

    // Nl x Ll samples for loop storage
    Input<Buffer<float, 2>> loop_storage_in{"loop_storage_in"};

    // Nl indices which indicate to which port each looper is mapped
    Input<Buffer<int32_t, 1>> ports_map{"ports_map"};

    // Nl indices which indicate soft and hard sync
    Input<Buffer<int32_t, 1>> hard_sync_map{"hard_sync_map"};
    Input<Buffer<int32_t, 1>> soft_sync_map{"soft_sync_map"};

    // Np indices which allow port inputs to get their samples from another
    // port instead of their own
    Input<Buffer<int32_t, 1>> port_input_override_map{"port_input_override_map"};

    // Np levels to indicate how much passthrough and volume we want on the ports
    Input<Buffer<float, 1>> port_passthrough_levels{"port_passthrough_levels"};
    Input<Buffer<float, 1>> port_volumes{"port_volumes"};

    // Np booleans to indicate whether each output port is muted
    Input<Buffer<int8_t, 1>> ports_muted{"ports_muted"};
    Input<Buffer<int8_t, 1>> port_inputs_muted{"port_inputs_muted"};

    // Nl levels to indicate how much playback volume we want on the loops
    Input<Buffer<float, 1>> loop_playback_volumes{"loop_playback_volumes"};

    // Nevents x Np amount of input timestamps (in number of frames relative to the start of this cycle).
    // The timestamps represent points in time where an event ocurred which we
    // may want to record separately from the audio data (e.g. MIDI).
    // The looping algorithm does not directly deal with these events, but provides:
    // - resulting loop storage indices
    //   to which these events may be associated (if we were recording at that time).
    // - boolean outputs which indicate whether the events are to be passed through directly
    //   to port outputs (e.g. MIDI monitoring).
    // We also have IDs for the events, to map them to the loops and remapped ports later.
    Input<Buffer<int32_t, 2>> port_event_timestamps_in{"port_event_timestamps_in"};
    Input<Buffer<int32_t, 1>> n_port_events{"n_port_events"};

    // Ns
    Input<int32_t> n_samples{"n_samples"};

    // Ll
    Input<int32_t> max_loop_length{"max_loop_length"};

    // The storage lock, if nonzero, will ensure that storage data and storage lengths
    // remain unchanged regardless of the loop state. Can be used to e.g. make sure
    // data is unaltered so that it can be read out thread-safe.
    Input<int8_t> storage_lock{"storage_lock"};

    // Ns x Np
    Output<Buffer<float, 2>> samples_out{"samples_out"};

    // Np
    Output<Buffer<float, 1>> port_input_peaks{"port_input_peaks"};
    Output<Buffer<float, 1>> port_output_peaks{"port_output_peaks"};
    // Nl
    Output<Buffer<float, 1>> loop_output_peaks{"loop_output_peaks"};

    // NLat x Np
    Output<Buffer<float, 2>> latencybuf_out{"latencybuf_out"};

    Output<int32_t> latencybuf_writepos_out{"latencybuf_writepos_out"};
    
    // Ns x Nl, intermediate result before output mixing
    Output<Buffer<float, 2>> samples_out_per_loop{"samples_out_per_loop"};

    // Nl
    Output<Buffer<int8_t, 1>> states_out{"states_out"};
    Output<Buffer<int32_t, 1>> positions_out{"positions_out"};
    Output<Buffer<int32_t, 1>> loop_lengths_out{"loop_lengths_out"};

    // Nl x 2
    Output<Buffer<int8_t, 2>> next_states_out{"next_states_out"};
    Output<Buffer<int8_t, 2>> next_state_countdowns_out{"next_state_countdowns_out"};

    // Nevents x Nl
    // event_recording_timestamps_out holds the timestamps in loop storage where each event
    // should be recorded, or -1 if not recorded at all.
    Output<Buffer<int32_t, 2>> event_recording_timestamps_out{"event_recording_timestamps_out"};

    // Nl x Ll
    Output<Buffer<float, 2>> loop_storage_out{"loop_storage_out"};

    void generate() {
        // Tell Halide what it can assume about all input buffer sizes
        {
            Expr lf = states_in.dim(0).min();
            Expr le = states_in.dim(0).extent();
            Expr bf = samples_in.dim(0).min();
            Expr be = samples_in.dim(0).extent();
            Expr sf = loop_storage_in.dim(0).min();
            Expr se = loop_storage_in.dim(0).extent();
            Expr pf = port_passthrough_levels.dim(0).min();
            Expr pe = port_passthrough_levels.dim(0).extent();

            // 1D buffers per loop
            next_states_in.dim(1).set_bounds(lf, le);
            next_state_countdowns_in.dim(1).set_bounds(lf, le);
            positions_in.dim(0).set_bounds(lf, le);
            loop_lengths_in.dim(0).set_bounds(lf, le);
            hard_sync_map.dim(0).set_bounds(lf, le);
            soft_sync_map.dim(0).set_bounds(lf, le);
            ports_map.dim(0).set_bounds(lf, le);
            loop_playback_volumes.dim(0).set_bounds(lf, le);

            // loop axis of 2D buffers
            loop_storage_in.dim(1).set_bounds(lf, le);

            // port axis of 2D buffers
            samples_in.dim(1).set_bounds(pf, pe);
            port_event_timestamps_in.dim(1).set_bounds(pf, pe);

            // port axis of 1D buffers
            port_volumes.dim(0).set_bounds(pf, pe);
            port_input_override_map.dim(0).set_bounds(pf, pe);
            ports_muted.dim(0).set_bounds(pf, pe);
            port_inputs_muted.dim(0).set_bounds(pf, pe);
            n_port_events.dim(0).set_bounds(pf, pe);

            // Constants
            next_states_in.dim(0).set_bounds(0, 2);
            next_state_countdowns_in.dim(0).set_bounds(0, 2);
        }

        Var loop("loop");
        Var port("port");
        Var x("x");

        // Boundary conditions help to vectorize
        Func _loop_lengths_in_b = repeat_edge(loop_lengths_in);
        Func _positions_in_b = repeat_edge(positions_in);
        Func _loop_storage_in = repeat_edge(loop_storage_in);
        Func _orig_samples_in = repeat_edge(samples_in);
        Func _soft_sync_map_b = repeat_edge(soft_sync_map);
        Func _hard_sync_map = repeat_edge(hard_sync_map);
        Func _port_input_override_map = repeat_edge(port_input_override_map);
        Func _states_in_b = repeat_edge(states_in);
        Func _next_states_in_b = repeat_edge(next_states_in);
        Func _next_state_countdowns_in_b = constant_exterior(next_state_countdowns_in, -1);
        Func _ports_muted = repeat_edge(ports_muted);
        Func _port_inputs_muted = repeat_edge(port_inputs_muted);
        Func _latencybuf_in = repeat_edge(latencybuf_in);
        Func _port_recording_latencies = repeat_edge(port_recording_latencies_in);
        Func _n_port_events_b = repeat_edge(n_port_events);
        Func _port_event_timestamps_in_b = repeat_edge(port_event_timestamps_in);
        Func _ports_map = repeat_edge(ports_map);

        // Do the port input overrides and port input mutes
        Func _muted_samples_in("_muted_samples_in");
        _muted_samples_in(x, port) = _orig_samples_in(x, port) *
            select(_port_inputs_muted(port) != 0, 0.0f, 1.0f);

        // For hard-linked loops, sneakily get the state data from the master loop
        Func _loop_lengths_in, _positions_in, _soft_sync_map, _states_in, _next_states_in,
             _next_state_countdowns_in;
        Expr mapped_loop = select(_hard_sync_map(loop) >= 0, _hard_sync_map(loop), loop);
        _loop_lengths_in(loop) = _loop_lengths_in_b(mapped_loop);
        _positions_in(loop) = _positions_in_b(mapped_loop);
        _soft_sync_map(loop) = select(
            _soft_sync_map_b(mapped_loop) == mapped_loop,
            loop, // If mapped loop is synced to self, pretend we are synced to ourselves too
            _soft_sync_map_b(mapped_loop)
        );
        _states_in(loop) = _states_in_b(mapped_loop);
        _next_states_in(x, loop) = _next_states_in_b(x, mapped_loop);
        _next_state_countdowns_in(x, loop) = _next_state_countdowns_in_b(x, mapped_loop);
        
        // Some definitions
        auto is_running_state = [](Expr state) { return state == Playing || state == PlayingMuted || state == Recording; };
        auto is_playing_state = [](Expr state) { return state == Playing || state == PlayingMuted; };
        auto clamp_to_storage = [&](Expr var) { return clamp(var, loop_storage_in.dim(0).min(), loop_storage_in.dim(0).max()); };

        // Latency buffer operations
        Expr n_frames = samples_in.dim(0).extent();
        Expr lbuf_size = latencybuf_in.dim(0).extent();
        RDom l(0, n_frames);
        latencybuf_out(x, port) = Halide::undef<float>();
        // Store samples into latency buffer
        latencybuf_out((latencybuf_writepos_in + l) % lbuf_size, port) = _muted_samples_in(l, port);
        Func _latencybuf_out = repeat_edge(latencybuf_out);
        // Update write index
        latencybuf_writepos_out() = (latencybuf_writepos_in + n_frames) % lbuf_size;

        // Apply port input overrides to redirect both samples, events and latencies
        Func _samples_in("_samples_in"), _port_event_timestamps_in{"_port_event_timestamps_in"},
             _n_port_events{"_n_port_events"};
        Expr remapped_port = _port_input_override_map(port);
        // TODO: using input directly here, could also take the latest from the latency buf.
        //       what is faster?
        _port_event_timestamps_in(x, port) = _port_event_timestamps_in_b(x, remapped_port);
        _n_port_events(port) = _n_port_events_b(remapped_port);
        _samples_in(x, port) = _muted_samples_in(x, remapped_port);
        Func _latency_applied_samples_in("_latency_applied_samples_in");
        Expr sample_idx = (latencybuf_writepos_in - _port_recording_latencies(remapped_port) + x) % lbuf_size;
        _latency_applied_samples_in(x, port) = _latencybuf_out(sample_idx, remapped_port);

        // Calculate input peaks
        RDom all_samples(0, samples_in.dim(0).extent());
        port_input_peaks(port) = argmax(abs(_samples_in(all_samples, port)))[1];

        // State transitions due to sync connections
        
        // Tells at which sample the loop will generate a soft sync.
        // For loops which will not generate a soft sync trigger, this
        // is set to -1.
        // Conditions which generate a soft sync:
        // - Starting to play from 0 (e.g. when Play pressed)
        // - Play wraps around
        // - Any state change from stopped or recording to stopped, recording or playing
        Func generates_soft_sync_at("generates_soft_sync_at");
        Expr is_starting_to_play = is_playing_state(_states_in(loop)) && _positions_in(loop) == 0;
        Expr will_play_beyond_end = is_playing_state(_states_in(loop)) && (_loop_lengths_in(loop) - _positions_in(loop)) <= n_samples;
        Expr will_play_beyond_end_from = (_loop_lengths_in(loop) - _positions_in(loop));
        Expr is_transitioning_immediately =
            is_starting_to_play ||
            (_states_in(loop) != _next_states_in(0, loop) && // TODO: is this correct for all loops, or just master?
             !is_playing_state(_states_in(loop)));
        // Debug prints
        // is_starting_to_play = print_when(is_starting_to_play, is_starting_to_play, loop, "starting, p:", _positions_in(loop), "l:", _loop_lengths_in(loop), "n:", n_samples);
        // will_play_beyond_end = print_when(will_play_beyond_end, will_play_beyond_end, loop, "wrapping, p:", _positions_in(loop), "l:", _loop_lengths_in(loop), "n:", n_samples);
        // Right-hand side
        Expr generates_soft_sync_at_expr =
            select(
                is_transitioning_immediately,
                0,
                select(
                    will_play_beyond_end,
                    will_play_beyond_end_from,
                    -1
                )
            );
        // More debug prints
        // _generates_soft_sync_at = print_when(is_starting_to_play || will_play_beyond_end, _generates_soft_sync_at, "is soft sync generated by", loop);
        // Func
        generates_soft_sync_at(loop) = generates_soft_sync_at_expr;
        Func _generates_soft_sync_at = repeat_edge(generates_soft_sync_at, { Range(positions_in.dim(0).min(), positions_in.dim(0).extent()) });
        
        // Update the state based on whether each loop will receive any
        // sync trigger this iteration.
        Expr is_soft_synced = 0 <= _soft_sync_map(loop) && _soft_sync_map(loop) <= states_in.dim(0).max();
        Expr my_master_loop = _soft_sync_map(loop);
        Expr will_receive_soft_sync = is_soft_synced && 
            _generates_soft_sync_at(my_master_loop) != -1;
        Expr state_change_trigger = will_receive_soft_sync || !is_soft_synced; // non-soft-synced loops always trigger immediately

        // Now deal with the next state countdown mechanics
        Expr state_will_transition = state_change_trigger && _next_state_countdowns_in(0, loop) == 0;
        next_state_countdowns_out(x, loop) = _next_state_countdowns_in(x, loop);
        // Decrement countdowns
        next_state_countdowns_out(0, loop) = select (
            state_change_trigger && _next_state_countdowns_in(0, loop) > 0,
            _next_state_countdowns_in(0, loop) - 1,
            Halide::undef<int8_t>()
        );
        // Shift to next state if needed
        next_state_countdowns_out(x, loop) = select(
            state_will_transition,
            _next_state_countdowns_in(x+1, loop), // Shift to next in queue
            Halide::undef<int8_t>()
        );
        next_states_out(x, loop) = select(
            state_will_transition,
            _next_states_in(x+1, loop),
            _next_states_in(x, loop)
        );
        Expr _new_state = select(
            state_will_transition,
            _next_states_in(0, loop),
            _states_in(loop)
        );
        // Debug prints
        // _new_state = print_when(move_to_next_state && _next_states_in(loop) != _states_in(loop),
        //     _new_state, "loop " , loop, "moving to next state: ", _new_state);
        // Func
        Func new_state("new_state");
        new_state(loop) = _new_state;
        
        // Determine sample ranges (w.r.t input/output) over which the looper will be in each running state.
        // Note: for these funcs, "playing" includes playing muted because much of the behavior is the same.
        // The additional "muted range" is just for determining whether to actually output samples later.
        Func playing_range("playing_range"), recording_range("recording_range"), muted_range("muted_range");
        // Some logic about playing state transitions
        Expr state_is_playing = _states_in(loop) == Playing || _states_in(loop) == PlayingMuted;
        Expr next_state_is_playing = _next_states_in(0, loop) == Playing || _next_states_in(0, loop) == PlayingMuted;
        Expr end_state_is_playing = new_state(loop) == Playing || _states_in(loop) == PlayingMuted;
        Expr will_start_playing = state_will_transition && !state_is_playing && next_state_is_playing;
        Expr will_stop_playing = state_will_transition && state_is_playing && !next_state_is_playing;
        Expr will_receive_soft_sync_at = _generates_soft_sync_at(my_master_loop);
        Expr will_transition_at = select(will_receive_soft_sync, will_receive_soft_sync_at, 0);
        // Determine the range.
        Expr playing_from = select(
            state_is_playing,
            0, // was already playing
            select(
                will_start_playing,
                will_transition_at,
                n_samples // will never start
            )
        );
        Expr playing_to = select(
            will_stop_playing,
            will_transition_at,
            select(
                end_state_is_playing,
                n_samples, // Note: not including last sample
                -1 // was always stopped
            )
        );
        Expr muted_from = select(
            _states_in(loop) == PlayingMuted,
            playing_from,
            select(
                new_state(loop) == PlayingMuted,
                will_transition_at,
                n_samples
            )
        );
        Expr muted_to = select(
            new_state(loop) == PlayingMuted,
            playing_to,
            select(
                _states_in(loop) == PlayingMuted,
                will_transition_at,
                -1
            )
        );
        playing_range(loop) = Tuple(playing_from, playing_to);
        muted_range(loop) = Tuple(muted_from, muted_to);

        // Some logic about recording state transitions
        Expr state_is_recording = _states_in(loop) == Recording;
        Expr next_state_is_recording = _next_states_in(0, loop) == Recording;
        Expr will_start_recording = state_will_transition && !state_is_recording && next_state_is_recording;
        Expr will_stop_recording = state_will_transition && state_is_recording && !next_state_is_recording;
        // Determine the range.
        Expr recording_from = select(
            state_is_recording,
            0, // was already playing
            select(
                will_start_recording,
                will_transition_at,
                n_samples // will never start
            )
        );
        Expr recording_to = select( // Note: not including last sample
            will_stop_recording,
            will_transition_at,
            select(
                new_state(loop) == Recording,
                n_samples,
                -1 // was always stopped
            )
        );
        recording_range(loop) = Tuple(recording_from, recording_to);

        // Per loop, store range where it will run at all.
        // Note that regardless of which state transition will happen,
        // there will always be at most one continuous running range.
        Func running_range("running_range");
        running_range(loop) = Tuple(
            min(playing_from, recording_from),
            max(playing_to, recording_to) // Note: not including last sample
        );

        // Define the domain over which we have something to do (play or record)
        RDom rr(0, n_samples, 0, states_in.dim(0).extent());
        rr.where(rr.x >= running_range(rr.y)[0] && rr.x <= running_range(rr.y)[1]);

        // We want to re-use some of the expressions above, but with the rr.y variable
        // instead of the loop variable. Make a convenience function for that.
        auto to_rr = [&](Expr e) { return Halide::Internal::substitute(loop, rr.y, e); };

        // Determine the relevant indices from/to which to read/write/record.
        // For recording, keep in mind:
        // - When we were already recording, continue at the position we were at
        // - When transitioning from another state to recording, we need to restart from 0
        Func rr_record_start_index("rr_record_start_index");
        rr_record_start_index(loop) = select(
            will_start_recording,
            0,
            _positions_in(loop)
        );
        Expr rr_record_index = clamp_to_storage(
            rr_record_start_index(rr.y) + rr.x - recording_range(rr.y)[0]
        );
        Expr rr_record_stop_index = rr_record_start_index(loop) + (recording_range(loop)[1] - recording_range(loop)[0]); // Not including last sample
        // For playing back from storage, keep in mind:
        // - If transitioning from recording, start from the recording end position
        // - Otherwise, just continue at the position we were at
        Func rr_playback_start_index("rr_playback_start_index");
        rr_playback_start_index(loop) = select(
            will_start_playing && will_stop_recording,
            0, //rr_record_stop_index,
            _positions_in(loop)
        );
        
        Expr will_wrap =
            will_play_beyond_end && (!is_soft_synced || will_receive_soft_sync);
        Expr will_wrap_from =
            select (
                is_soft_synced,
                will_receive_soft_sync_at,
                will_play_beyond_end_from
            );

        Expr rr_playback_index_until_end_part =
            Halide::min(
                (rr_playback_start_index(rr.y) + rr.x),
                _loop_lengths_in(rr.y) - 1
            ); // Play until end of length and "hang" there
        Expr rr_playback_index_wrapped_part =
            select (
                will_wrap,
                Halide::max(0, rr.x - will_wrap_from + 1),
                0
            );
        Expr rr_playback_index =  to_rr ( clamp_to_storage(
            (rr_playback_index_until_end_part + rr_playback_index_wrapped_part) % _loop_lengths_in(rr.y) // Wrapping around again just in case len < n_frames
        ));

        // Compute stored recorded samples for each loop
        Expr is_recording = rr.x >= recording_range(rr.y)[0] && rr.x < recording_range(rr.y)[1];
        loop_storage_out(x, loop) = Halide::undef<float>();
        loop_storage_out(rr_record_index, rr.y) = select(
            is_recording && storage_lock == 0,
            _latency_applied_samples_in(rr.x, _ports_map(rr.y)),
            Halide::undef<float>() // No recording
        );

        // Calculate which events should be recorded and where in the loops.
        RDom event(0, port_event_timestamps_in.dim(0).extent());
        Expr loop_input_port = _port_input_override_map(ports_map(loop));
        event.where(event.x < _n_port_events(loop_input_port));
        event_recording_timestamps_out(x, loop) = Halide::undef<int32_t>();
        Expr event_in_timestamp = _port_event_timestamps_in(event, loop_input_port);
        // We already calculated the recording index in the loop before, but we used
        // the rr domain for that. Use variable substitution to repurpose the same expression
        // for events too.
        Expr event_recording_index = Halide::Internal::substitute(
            rr.y, loop,
            Halide::Internal::substitute(
                rr.x, event_in_timestamp,
                rr_record_index
            )
        );
        Expr event_in_recording_range =
            event_in_timestamp >= recording_range(loop)[0] && event_in_timestamp < recording_range(loop)[1];
        event_recording_timestamps_out(event, loop) = select(
            event_in_recording_range && storage_lock == 0,
            event_recording_index,
            -1
        );

        // Compute output samples for each loop
        samples_out_per_loop(x, loop) = 0.0f;
        Expr is_playing = rr.x >= playing_range(rr.y)[0] && rr.x < playing_range(rr.y)[1];
        Expr is_muted = rr.x >= muted_range(rr.y)[0] && rr.x < muted_range(rr.y)[1];
        samples_out_per_loop(rr.x, rr.y) = 
            to_rr (select(
                is_playing && !is_muted,
                _loop_storage_in(rr_playback_index, rr.y) * loop_playback_volumes(rr.y),
                Halide::undef<float>() // No playback
            ));

        // Compute output peaks per loop
        loop_output_peaks(loop) = argmax(abs(samples_out_per_loop(all_samples, loop)))[1];

        // Compute output samples mixed into ports
        samples_out(x, port) = _samples_in(x, port) * port_passthrough_levels(port);
        RDom l2p(0, n_samples, ports_map.dim(0).min(), ports_map.dim(0).extent());
        // No need to mix in looper samples for non-playing loopers
        l2p.where(is_playing_state(_states_in(l2p.y)) || is_playing_state(new_state(l2p.y)));
        samples_out(l2p.x, clamp(
            ports_map(l2p.y),
            samples_out.dim(1).min(),
            samples_out.dim(1).max()
        )) += samples_out_per_loop(l2p.x, l2p.y);
        samples_out(x, port) = samples_out(x, port) *
            port_volumes(port) *
            select(_ports_muted(port) != 0, 0.0f, 1.0f);
        
        // Compute output peaks per port
        port_output_peaks(port) = argmax(abs(samples_out(all_samples, port)))[1];

        // Compute output states
        states_out(loop) = new_state(loop);

        // Compute output lengths
        Expr n_recorded_samples = max(0, recording_range(loop)[1] - recording_range(loop)[0]);
        loop_lengths_out(loop) = select(
            storage_lock == 0,
                select(will_start_recording,
                       n_recorded_samples,
                       _loop_lengths_in(loop) + n_recorded_samples),
            _loop_lengths_in(loop)
        );
        
        // Compute output positions
        // TODO: This should be determined based on playback index/recording index/static index
        
        Expr n_played_samples = max(0, playing_range(loop)[1] - playing_range(loop)[0]);
        Expr old_version = select(
            will_start_recording,
            (_positions_in(loop) + n_played_samples) % loop_lengths_in(loop) + n_recorded_samples, // Wrap then record
            (_positions_in(loop) + n_recorded_samples + n_played_samples) % loop_lengths_out(loop)           // Record then wrap
        );

        positions_out(loop) = Halide::Internal::substitute(rr.y, loop, select(
            is_playing_state(states_out(loop)),
            Halide::Internal::substitute(rr.x, n_samples, rr_playback_index) % _loop_lengths_in(loop),
            old_version
        ));

        // Schedule
        new_state.compute_root();
        loop_lengths_out.compute_root();
        positions_out.compute_root();
        states_out.compute_root();
        running_range.compute_root();
        playing_range.compute_root();
        recording_range.compute_root();
        generates_soft_sync_at.compute_root();
        rr_playback_start_index.compute_root();
        rr_record_start_index.compute_root();
        latencybuf_out.compute_root();
        latencybuf_writepos_out.compute_root();
        port_input_peaks.compute_root();
        port_output_peaks.compute_root();
        loop_output_peaks.compute_root();
        next_states_out.compute_root();
        next_state_countdowns_out.compute_root();
        // _loop_lengths_in.compute_root();
        // _positions_in.compute_root();
        // _soft_sync_map.compute_root();
        // _states_in.compute_root();
        // _next_states_in.compute_root();
        
        if(true) { //x inside schedule
            loop_storage_out
                .update()
                .reorder(rr.x, rr.y)
                //.allow_race_conditions()
                //.vectorize(rr.x, 8)
                //.compute_with(samples_out_per_loop.update(), rr.x)
                //.trace_stores()
                ;

            samples_out_per_loop.compute_root();
            samples_out_per_loop
                .update()
                .reorder(rr.x, rr.y)
                //.allow_race_conditions()
                //.vectorize(rr.x, 8)
                //.trace_stores()
                ;
        }

        samples_out
            //.vectorize(x, 8)
            ;
        samples_out
            .update(0)
            //.vectorize(x, 8)
            ;
        samples_out
            .update(1)
            //.vectorize(x, 8)
            ;
        
        Pipeline({samples_out, loop_storage_out, states_out, positions_out, loop_lengths_out}).print_loop_nest();
    }
};

}  // namespace

HALIDE_REGISTER_GENERATOR(Loops, shoopdaloop_loops)