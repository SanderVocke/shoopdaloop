#include "Halide.h"
#include "shoopdaloop_backend.h"

namespace {

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
        Var loop("loop"), port("port"), x("x");

        Expr first_loop = 0;
        Expr last_loop = states_in.dim(0).max();
        Expr first_port = 0;
        Expr last_port = samples_in.dim(1).max();
        Expr first_sample = 0;
        Expr last_sample = n_samples-1;

        // State never changes during processing
        Func state_in("state_in"), next_state_in("next_state_in"), new_state("new_state");
        state_in(loop) = select(loop >= first_loop && loop <= last_loop,
            states_in(clamp(loop, first_loop, last_loop)), 0);
        next_state_in(loop) = select(loop >= first_loop && loop <= last_loop,
            next_states_in(loop), 0);
        
        Func soft_sync("soft_sync"), hard_sync("hard_sync");
        soft_sync(loop) = select(loop >= first_loop && loop <= last_loop,
            soft_sync_map(loop), 0);
        hard_sync(loop) = select(loop >= first_loop && loop <= last_loop,
            hard_sync_map(loop), 0);

        // Boundary conditions help to vectorize
        // TODO: use pre-baked ones
        Func len_in("len_in");
        len_in(loop) = select(loop >= first_loop && loop <= last_loop,
            loop_lengths_in(clamp(loop, first_loop, last_loop)), 0);
        
        Func pos_in("pos_in");
        pos_in(loop) = select(loop >= first_loop && loop <= last_loop,
            positions_in(clamp(loop, first_loop, last_loop)), 0);

        Func storage_in("storage_in");
        storage_in(x, loop) = select(loop >= first_loop && loop <= last_loop,
            select(x >= first_sample && x <= last_sample,
                loop_storage_in(x, loop), 0), 0);

        // Map input samples per port to input samples for each loop
        Func _samples_in("_samples_in");
        Expr buf = clamp(ports_map(loop), first_port, last_port);
        _samples_in(x, loop) = select(buf >= first_port && buf <= last_port,
            select(x >= first_sample && x <= last_sample,
                samples_in(x, buf), 0), 0);
        
        // Some definitions
        auto is_running_state = [](Expr state) { return state == Playing || state == PlayingMuted || state == Recording; };
        auto may_generate_sync_state = [](Expr state) { return state == Playing || state == PlayingMuted; };
        
        // State transitions due to sync connections
        
        // Tells at which sample the loop will generate a soft sync.
        // For loops which will not generate a soft sync trigger, this
        // is set to -1.
        Func generates_soft_sync_at("generates_soft_sync_at");
        Expr is_starting = may_generate_sync_state(state_in(loop)) && pos_in(loop) == 0;
        Expr will_wrap = may_generate_sync_state(state_in(loop)) && (len_in(loop) - pos_in(loop)) < n_samples;
        // Debug prints
        // is_starting = print_when(is_starting, is_starting, loop, "starting, p:", pos_in(loop), "l:", len_in(loop), "n:", n_samples);
        // will_wrap = print_when(will_wrap, will_wrap, loop, "wrapping, p:", pos_in(loop), "l:", len_in(loop), "n:", n_samples);
        // Right-hand side
        Expr _generates_soft_sync_at =
            select(
                is_starting,
                0,
                select(
                    will_wrap,
                    len_in(loop) - pos_in(loop),
                    -1
                )
            );
        // More debug prints
        // _generates_soft_sync_at = print_when(is_starting || will_wrap, _generates_soft_sync_at, "is soft sync generated by", loop);
        // Func
        generates_soft_sync_at(loop) = _generates_soft_sync_at;
        
        // Update the state based on whether each loop will receive any
        // sync trigger this iteration.
        Expr is_soft_synced = 0 <= soft_sync(loop) && soft_sync(loop) <= states_in.dim(0).max();
        Expr my_master_loop = clamp(soft_sync(loop), first_loop, last_loop);
        Expr will_receive_soft_sync = is_soft_synced && 
            generates_soft_sync_at(my_master_loop) != -1;
        Expr move_to_next_state = will_receive_soft_sync || !is_soft_synced; // non-soft-synced loops always move to next state immediately
        // Right-hand side
        Expr _new_state = select(
            move_to_next_state,
            next_state_in(loop),
            state_in(loop)
        );
        // Debug prints
        // _new_state = print_when(move_to_next_state && next_state_in(loop) != state_in(loop),
        //     _new_state, "loop " , loop, "moving to next state: ", _new_state);
        // Func
        new_state(loop) = _new_state;
        
        // Tells at which sample, if any, each loop will start running.
        // This applies to both playback and recording.
        // -1 means the loop will not run at all this iteration.
        Func will_run_from("will_run_from");
        Expr was_already_running = is_running_state(state_in(loop));
        Expr will_start_running_due_to_soft_sync = 
            !is_running_state(state_in(loop)) && will_receive_soft_sync;
        Expr soft_sync_received_at = generates_soft_sync_at(my_master_loop);
        will_run_from(loop) =
            select(
                was_already_running,
                0,
                select(
                    will_start_running_due_to_soft_sync,
                    soft_sync_received_at,
                    -1
                )
            );

        // Compute new loop lengths and positions due to recording
        Func len("len"), end_pos("end_pos"), n_samples_ran("n_samples_ran");
        n_samples_ran(loop) = select(
            will_run_from(loop) >= 0,
            n_samples - will_run_from(loop),
            0
        );
        len(loop) = select(
            new_state(loop) == Recording,
            min(len_in(loop) + n_samples_ran(loop), max_loop_length),
            len_in(loop)
        );
        end_pos(loop) = (pos_in(loop) + n_samples_ran(loop)) % len(loop); // TODO what will happen when max length reached?

        // Store loop data
        RDom rr(0, n_samples, 0, states_in.dim(0).extent());
        rr.where(is_running_state(new_state(rr.y))); // TODO: handle stops in middle of iteration
        rr.where(rr.x >= will_run_from(rr.y));

        Expr rr_storage_index = clamp(
            pos_in(rr.y) + rr.x - will_run_from(rr.y),
            loop_storage_in.dim(0).min(),
            loop_storage_in.dim(0).max()
        );
        Expr rr_buf_index = clamp(
            rr.x - will_run_from(rr.y),
            first_sample,
            last_sample
        );
        Func storage_out("storage_out");
        {
            loop_storage_out(x, loop) = Halide::undef<float>();
            loop_storage_out(rr_storage_index, rr.y) = select(
                new_state(rr.y) == Recording && // Recording?
                    will_run_from(rr.y) != -1,// && // Will run?
                    //rr.x >= will_run_from(rr.y),// ...And past the starting point?
                _samples_in(rr_buf_index, rr.y),
                storage_in(rr_storage_index, rr.y)
            );
        }

        // Compute output samples for each loop
        samples_out_per_loop(x, loop) = 0.0f;
        samples_out_per_loop(rr_buf_index, rr.y) = 
            select(
                //will_run_from(rr.y) != -1 && // Will run?
                //rr.x >= will_run_from(rr.y) && // ...And past the starting point?
                new_state(rr.y) == Playing, // ...And playing?
                storage_in(rr_storage_index, rr.y) * loop_playback_volumes(rr.y), // Playback a sample
                Halide::undef<float>() // Don't playback
            );

        // Compute output samples mixed into ports
        samples_out(x, port) = _samples_in(x, port) * port_passthrough_levels(port);
        RDom ports(ports_map.dim(0).min(), ports_map.dim(0).extent());
        samples_out(x, clamp(
            ports_map(ports),
            samples_out.dim(1).min(),
            samples_out.dim(1).max()
        )) += samples_out_per_loop(x, ports);
        samples_out(x, port) = samples_out(x, port) * port_volumes(port);

        // Compute output states
        states_out(loop) = new_state(loop);

        // Compute output lengths
        loop_lengths_out(loop) = len(loop);

        // Compute output positions
        positions_out(loop) = end_pos(loop);

        // Schedule
        new_state.compute_root();
        will_run_from.compute_root();
        pos_in.compute_root();
        len_in.compute_root();
        generates_soft_sync_at.compute_root();
        
        if(true) { //x inside schedule
            loop_storage_out
                .update()
                .reorder(rr.x, rr.y)
                .allow_race_conditions()
                //.vectorize(rr.x, 8)
                .compute_with(samples_out_per_loop.update(), rr.x)
                //.trace_stores()
                ;

            samples_out_per_loop.compute_root();
            samples_out_per_loop
                .update()
                .reorder(rr.x, rr.y)
                .allow_race_conditions()
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