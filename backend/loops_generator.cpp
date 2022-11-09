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

    // Live input samples. Ns samples by Np ports
    Input<Buffer<float, 2>> samples_in{"samples_in"};

    // Nl states, positions, lengths (one for each looper)
    Input<Buffer<int8_t, 1>> states_in{"states_in"};
    Input<Buffer<int8_t, 1>> next_states_in{"next_states_in"};
    Input<Buffer<int32_t, 1>> positions_in{"positions_in"};
    Input<Buffer<int32_t, 1>> loop_lengths_in{"loop_lengths_in"};

    // Nl x Ll samples for loop storage
    Input<Buffer<float, 2>> loop_storage_in{"loop_storage_in"};

    // Nl indices which indicate to which port each looper is mapped
    Input<Buffer<int32_t, 1>> ports_map{"ports_map"};

    // Nl indices which indicate soft and hard sync
    Input<Buffer<int32_t, 1>> hard_sync_map{"hard_sync_map"};
    Input<Buffer<int32_t, 1>> soft_sync_map{"soft_sync_map"};

    // Np levels to indicate how much passthrough and volume we want on the ports
    Input<Buffer<float, 1>> port_passthrough_levels{"port_passthrough_levels"};
    Input<Buffer<float, 1>> port_volumes{"port_volumes"};

    // Nl levels to indicate how much playback volume we want on the loops
    Input<Buffer<float, 1>> loop_playback_volumes{"loop_playback_volumes"};

    // Ns
    Input<int32_t> n_samples{"n_samples"};

    // Ll
    Input<int32_t> max_loop_length{"max_loop_length"};

    // Ns x Np
    Output<Buffer<float, 2>> samples_out{"samples_out"};

    // Ns x Nl, intermediate result before output mixing
    Output<Buffer<float, 2>> samples_out_per_loop{"samples_out_per_loop"};

    // Nl
    Output<Buffer<int8_t, 1>> states_out{"states_out"};
    Output<Buffer<int32_t, 1>> positions_out{"positions_out"};
    Output<Buffer<int32_t, 1>> loop_lengths_out{"loop_lengths_out"};

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
            next_states_in.dim(0).set_bounds(lf, le);
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

            // port axis of 1D buffers
            port_volumes.dim(0).set_bounds(pf, pe);
        }

        Var loop("loop");
        Var port("port");
        Var x("x");

        // Boundary conditions help to vectorize
        Func _loop_lengths_in_b = repeat_edge(loop_lengths_in);
        Func _positions_in_b = repeat_edge(positions_in);
        Func _loop_storage_in = repeat_edge(loop_storage_in);
        Func _samples_in = repeat_edge(samples_in);
        Func _soft_sync_map_b = repeat_edge(soft_sync_map);
        Func _hard_sync_map = repeat_edge(hard_sync_map);
        Func _states_in_b = repeat_edge(states_in);
        Func _next_states_in_b = repeat_edge(next_states_in);

        // For hard-linked loops, sneakily get the state data from the master loop
        Func _loop_lengths_in, _positions_in, _soft_sync_map, _states_in, _next_states_in;
        Expr mapped_loop = select(_hard_sync_map(loop) >= 0, _hard_sync_map(loop), loop);
        _loop_lengths_in(loop) = _loop_lengths_in_b(mapped_loop);
        _positions_in(loop) = _positions_in_b(mapped_loop);
        _soft_sync_map(loop) = _soft_sync_map_b(mapped_loop);
        _states_in(loop) = _states_in_b(mapped_loop);
        _next_states_in(loop) = _next_states_in_b(mapped_loop);
        
        // Some definitions
        auto is_running_state = [](Expr state) { return state == Playing || state == PlayingMuted || state == Recording; };
        auto is_playing_state = [](Expr state) { return state == Playing || state == PlayingMuted; };
        auto clamp_to_storage = [&](Expr var) { return clamp(var, loop_storage_in.dim(0).min(), loop_storage_in.dim(0).max()); };

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
        Expr will_wrap = is_playing_state(_states_in(loop)) && (_loop_lengths_in(loop) - _positions_in(loop)) < n_samples;
        Expr is_transitioning_immediately =
            is_starting_to_play ||
            (_states_in(loop) != _next_states_in(loop) &&
             !is_playing_state(_states_in(loop)));
        // Debug prints
        // is_starting_to_play = print_when(is_starting_to_play, is_starting_to_play, loop, "starting, p:", _positions_in(loop), "l:", _loop_lengths_in(loop), "n:", n_samples);
        // will_wrap = print_when(will_wrap, will_wrap, loop, "wrapping, p:", _positions_in(loop), "l:", _loop_lengths_in(loop), "n:", n_samples);
        // Right-hand side
        Expr generates_soft_sync_at_expr =
            select(
                is_transitioning_immediately,
                0,
                select(
                    will_wrap,
                    _loop_lengths_in(loop) - _positions_in(loop),
                    -1
                )
            );
        // More debug prints
        // _generates_soft_sync_at = print_when(is_starting_to_play || will_wrap, _generates_soft_sync_at, "is soft sync generated by", loop);
        // Func
        generates_soft_sync_at(loop) = generates_soft_sync_at_expr;
        Func _generates_soft_sync_at = repeat_edge(generates_soft_sync_at, { Range(positions_in.dim(0).min(), positions_in.dim(0).extent()) });
        
        // Update the state based on whether each loop will receive any
        // sync trigger this iteration.
        Expr is_soft_synced = 0 <= _soft_sync_map(loop) && _soft_sync_map(loop) <= states_in.dim(0).max();
        Expr my_master_loop = _soft_sync_map(loop);
        Expr will_receive_soft_sync = is_soft_synced && 
            _generates_soft_sync_at(my_master_loop) != -1;
        Expr move_to_next_state = will_receive_soft_sync || !is_soft_synced; // non-soft-synced loops always move to next state immediately
        // Right-hand side
        Expr _new_state = select(
            move_to_next_state,
            _next_states_in(loop),
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
        Expr next_state_is_playing = _next_states_in(loop) == Playing || _next_states_in(loop) == PlayingMuted;
        Expr end_state_is_playing = new_state(loop) == Playing || _states_in(loop) == PlayingMuted;
        Expr will_start_playing = will_receive_soft_sync && !state_is_playing && next_state_is_playing;
        Expr will_stop_playing = will_receive_soft_sync && state_is_playing && !next_state_is_playing;
        Expr will_transition_at = _generates_soft_sync_at(my_master_loop);
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
        Expr next_state_is_recording = _next_states_in(loop) == Recording;
        Expr will_start_recording = will_receive_soft_sync && !state_is_recording && next_state_is_recording;
        Expr will_stop_recording = will_receive_soft_sync && state_is_recording && !next_state_is_recording;
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
        // It can be derived that regardless of which state transition will happen,
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
            rr_record_stop_index,
            _positions_in(loop)
        );
        Expr rr_playback_index = clamp_to_storage(
            (rr_playback_start_index(rr.y) + rr.x) % loop_storage_in.dim(0).extent() // Wrap around
        );

        // Compute stored recorded samples for each loop
        Expr is_recording = rr.x >= recording_range(rr.y)[0] && rr.x < recording_range(rr.y)[1]; 
        loop_storage_out(x, loop) = Halide::undef<float>();
        loop_storage_out(rr_record_index, rr.y) = select(
            is_recording,
            _samples_in(rr.x, rr.y),
            Halide::undef<float>() // No recording
        );

        // Compute output samples for each loop
        samples_out_per_loop(x, loop) = 0.0f;
        Expr is_playing = rr.x >= playing_range(rr.y)[0] && rr.x < playing_range(rr.y)[1];
        Expr is_muted = rr.x >= muted_range(rr.y)[0] && rr.x < muted_range(rr.y)[1];
        samples_out_per_loop(rr.x, rr.y) = 
            select(
                is_playing && !is_muted,
                _loop_storage_in(rr_playback_index, rr.y) * loop_playback_volumes(rr.y),
                Halide::undef<float>() // No playback
            );

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
        samples_out(x, port) = samples_out(x, port) * port_volumes(port);

        // Compute output states
        states_out(loop) = new_state(loop);

        // Compute output lengths
        Expr n_recorded_samples = max(0, recording_range(loop)[1] - recording_range(loop)[0]);
        loop_lengths_out(loop) = select(
            will_start_recording,
            n_recorded_samples,
            _loop_lengths_in(loop) + n_recorded_samples
        );

        // Compute output positions
        Expr n_played_samples = max(0, playing_range(loop)[1] - playing_range(loop)[0]);
        positions_out(loop) = select(
            will_start_recording,
            (_positions_in(loop) + n_played_samples) % loop_lengths_in(loop) + n_recorded_samples, // Wrap then record
            (_positions_in(loop) + n_recorded_samples + n_played_samples) % loop_lengths_out(loop)           // Record then wrap
        );

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