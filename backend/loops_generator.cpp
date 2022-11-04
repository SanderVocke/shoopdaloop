#include "Halide.h"
#include "types.hpp"

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
    Input<Buffer<int32_t, 1>> positions_in{"positions_in"};
    Input<Buffer<int32_t, 1>> loop_lengths_in{"loop_lengths_in"};

    // Nl x Ll samples for loop storage
    Input<Buffer<float, 2>> loop_storage_in{"loop_storage_in"};

    // Nl indices which indicate to which port each looper is mapped
    Input<Buffer<int32_t, 1>> ports_map{"ports_map"};

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

        // State never changes during processing
        Func state("state");
        state(loop) = select(loop >= states_in.dim(0).min() && loop <= states_in.dim(0).max(),
            states_in(loop), 0);

        // Boundary conditions help to vectorize
        Expr len_in = select(loop >= loop_lengths_in.dim(0).min() && loop <= loop_lengths_in.dim(0).max(),
            loop_lengths_in(loop), 0);
        
        Func pos_in("pos_in");
        pos_in(loop) = select(loop >= positions_in.dim(0).min() && loop <= positions_in.dim(0).max(),
            positions_in(loop), 0);

        Func storage_in("storage_in");
        storage_in(x, loop) = select(loop >= loop_storage_in.dim(1).min() && loop <= loop_storage_in.dim(1).max(),
            select(x >= loop_storage_in.dim(0).min() && x <= loop_storage_in.dim(0).max(),
                loop_storage_in(x, loop), 0), 0);

        // Map input samples per port to input samples for each loop
        Func _samples_in("_samples_in");
        Expr buf = clamp(ports_map(loop), samples_in.dim(1).min(), samples_in.dim(1).max());
        _samples_in(x, loop) = select(buf >= samples_in.dim(0).min() && buf <= samples_in.dim(0).max(),
            select(x >= samples_in.dim(0).min() && x <= samples_in.dim(0).max(),
                samples_in(x, buf), 0), 0);

        // Compute new loop lengths due to recording
        Func len("len");
        len(loop) = Halide::select(
            state(loop) == Recording,
            min(len_in + n_samples, max_loop_length),
            len_in);

        // Store loop data
        RDom rr(0, n_samples);
        Expr rr_storage_index = clamp(
                pos_in(loop) + rr,
                loop_storage_in.dim(1).min(),
                loop_storage_in.dim(1).max()
            );
        Func storage_out("storage_out");
        {
            loop_storage_out(x, loop) = Halide::undef<float>();
            Expr sample_index = rr;
            loop_storage_out(rr_storage_index, loop) = select(
                state(loop) == Recording,
                _samples_in(sample_index, loop),
                storage_in(rr.x, rr_storage_index)
            );
        }

        // Compute output samples for each loop
        {
            Expr storage_index = clamp(
                pos_in(loop) + rr,
                loop_storage_in.dim(0).min(),
                loop_storage_in.dim(0).max()
            );
            samples_out_per_loop(x, loop) = Halide::undef<float>();
            samples_out_per_loop(clamp(
                rr,
                samples_out_per_loop.dim(0).min(),
                samples_out_per_loop.dim(0).max()
            ), loop) = 
                select(
                    states_in(loop) == Playing,
                    storage_in(storage_index, loop) * loop_playback_volumes(loop),
                    0.0f
                );
        }

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
        states_out(loop) = state(loop);

        // Compute output positions
        positions_out(loop) = select(
            states_in(loop) == Playing || states_in(loop) == Recording,
            (positions_in(loop) + n_samples) % loop_lengths_in(loop),
            positions_in(loop)
        );

        // Compute output lengths
        loop_lengths_out(loop) = len(loop);

        // Schedule        
        loop_storage_out
            .update()
            .reorder(rr, loop)
            .allow_race_conditions()
            .vectorize(rr, 8)
            .compute_with(samples_out_per_loop.update(), rr)
            //.trace_stores()
            ;

        samples_out_per_loop.compute_root();
        samples_out_per_loop
            .update()
            .reorder(rr, loop)
            .allow_race_conditions()
            .vectorize(rr, 8)
            //.trace_stores()
            ;
        
        //samples_out.trace_stores();
        //positions_out.trace_stores();
        
        Pipeline({samples_out, loop_storage_out, states_out, positions_out, loop_lengths_out}).print_loop_nest();
    }
};

}  // namespace

HALIDE_REGISTER_GENERATOR(Loops, shoopdaloop_loops)