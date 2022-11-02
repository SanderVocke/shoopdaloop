#include "Halide.h"
#include "types.hpp"

namespace {

class Loops : public Halide::Generator<Loops> {
public:
    Input<Buffer<float, 2>> samples_in{"samples_in"};
    Input<Buffer<int8_t, 1>> states_in{"states_in"};
    Input<Buffer<int32_t, 1>> positions_in{"positions_in"};
    Input<Buffer<int32_t, 1>> loop_lengths_in{"loop_lengths_in"};
    Input<Buffer<float, 2>> loop_storage_in{"loop_storage_in"};

    Input<int32_t> n_samples{"n_samples"};
    Input<int32_t> max_loop_length{"max_loop_length"};

    Output<Buffer<float, 2>> samples_out{"samples_out"};
    Output<Buffer<int8_t, 1>> states_out{"states_out"};
    Output<Buffer<int32_t, 1>> positions_out{"positions_out"};
    Output<Buffer<int32_t, 1>> loop_lengths_out{"loop_lengths_out"};
    Output<Buffer<float, 2>> loop_storage_out{"loop_storage_out"};

    void generate() {
        Var loop("loop"), x("x");

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
        storage_in(loop, x) = select(loop >= loop_storage_in.dim(0).min() && loop <= loop_storage_in.dim(0).max(),
            select(x >= loop_storage_in.dim(1).min() && x <= loop_storage_in.dim(1).max(),
                loop_storage_in(loop, x), 0), 0);

        Func _samples_in("_samples_in");
        _samples_in(loop, x) = select(loop >= samples_in.dim(0).min() && loop <= samples_in.dim(0).max(),
            select(x >= samples_in.dim(1).min() && x <= samples_in.dim(1).max(),
                samples_in(loop, x), 0), 0);

        // Compute new loop lengths due to recording
        Func len("len");
        len(loop) = Halide::select(
            state(loop) == Recording,
            max(len_in + 1, max_loop_length),
            len_in);

        // Compute new positions due to playback/recording
        // 1. pure definition (unchanged)
        Func pos("pos");
        pos(loop, x) = pos_in(loop);
        // 2. update chronologically
        RDom r(1, n_samples);
        Expr next_pos = (pos(loop, r-1) + 1) % len(loop);
        pos(loop, r) = select(
                state(loop) == Playing || state(loop) == Recording,
                next_pos,
                pos(loop, r-1)
        );

        Func pos_base("pos_base");
        pos_base(loop, x) = select(
            state(loop) == Playing || state(loop) == Recording,
            x,
            0
        );

        // Store loop data
        RDom rr(0, n_samples);
        Expr rr_storage_index = clamp(
                pos_in(loop) + rr,
                loop_storage_in.dim(1).min(),
                loop_storage_in.dim(1).max()
            );
        Func storage_out("storage_out");
        {
            loop_storage_out(loop, x) = Halide::undef<float>();
            Expr sample_index = rr;
            loop_storage_out(loop, rr_storage_index) = select(
                state(loop) == Recording,
                _samples_in(loop, sample_index),
                Halide::undef<float>() // storage_in(rr.x, rr_storage_index)
            );
        }

        // Compute output samples
        {
            Expr storage_index = clamp(
                pos_base(loop, rr) + positions_in(loop),
                loop_storage_in.dim(1).min(),
                loop_storage_in.dim(1).max()
            );
            samples_out(loop, x) = Halide::undef<float>();
            samples_out(loop, clamp(
                rr,
                samples_out.dim(0).min(),
                samples_out.dim(0).max())
            ) = storage_in(loop, storage_index);
        }

        // Compute output states
        states_out(loop) = state(loop);

        // Compute output positions
        positions_out(loop) = pos_base(loop, n_samples-1);

        // Compute output lengths
        loop_lengths_out(loop) = len(loop);

        // loop_storage_out
        //     .compute_at(samples_out, x);
        //     ;
        // loop_storage_out.update()
        //     .reorder(rr.x, rr.y)
        //     .vectorize(rr.x, 8)
        //     ;
        
        // samples_out.reorder(loop, x)
        //     .reorder(loop, x)
        //     .vectorize(loop, 8)
        //     ;

        loop_storage_out
            //.compute_root()
            //.compute_at(samples_out, loop);
            ;
        
        loop_storage_out
            .update()
            .reorder(rr, loop)
            .allow_race_conditions()
            .vectorize(rr, 8)
            .compute_with(samples_out.update(), rr);
            //.allow_race_conditions()
            //.reorder(rr.x, rr.y)
            //.vectorize(rr.x, 8)
            //.vectorize(rr.y, 8)
            //.unroll(rr.y, 64)
            ;

        samples_out
            .update()
            .reorder(rr, loop)
            .allow_race_conditions()
            .vectorize(rr, 8)
            //.vectorize(loop, 8)
                  //.reorder(loop, x)
                  //.vectorize(x, 8)
                  //.vectorize(loop, 8)
                   ;
        Pipeline({samples_out, loop_storage_out}).print_loop_nest();
    }
};

}  // namespace

HALIDE_REGISTER_GENERATOR(Loops, shoopdaloop_loops)