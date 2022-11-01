#include "Halide.h"
#include "types.hpp"

namespace {

class Loops : public Halide::Generator<Loops> {
public:
    Input<Buffer<float, 2>> samples_in{"samples_in"};
    Input<Buffer<uint8_t, 1>> states_in{"states_in"};
    Input<Buffer<uint16_t, 1>> positions_in{"positions_in"};
    Input<Buffer<uint16_t, 1>> loop_lengths_in{"loop_lengths_in"};
    Input<Buffer<float, 2>> loop_storage_in{"loop_storage_in"};

    Input<uint16_t> n_samples{"n_samples"};
    Input<uint16_t> max_loop_length{"max_loop_length"};

    Output<Buffer<float, 2>> samples_out{"samples_out"};
    Output<Buffer<uint8_t, 1>> states_out{"states_out"};
    Output<Buffer<uint16_t, 1>> positions_out{"positions_out"};
    Output<Buffer<uint16_t, 1>> loop_lengths_out{"loop_lengths_out"};
    Output<Buffer<float, 2>> loop_storage_out{"loop_storage_out"};

    void generate() {
        Func pos("pos"), state("state"), len("len"), len_in("len_in"), storage_out("storage_out");
        Var loop("loop"), x("x");
        RDom r(1, n_samples);

        // State never changes during processing
        state(loop) = states_in(clamp(loop, 0, states_in.dim(0).extent()-1));

        // Boundary conditions help to vectorize
        len_in(loop) = loop_lengths_in(clamp(loop, 0, loop_lengths_in.dim(0).extent()-1));

        // Compute new loop lengths due to recording
        len(loop) = Halide::select(
            state(loop) == Recording,
            max(len_in(loop) + 1, max_loop_length),
            len_in(loop));

        // Compute new positions due to playback/recording
        // 1. pure definition (unchanged)
        pos(loop, x) = positions_in(loop);
        // 2. update chronologically
        Expr next_pos = (pos(loop, r-1) + 1) % len(loop);
        pos(loop, r) = clamp(
            select(
                state(loop) == Playing || state(loop) == Recording,
                next_pos,
                pos(loop, r-1)
            ),
            0, min(len(loop), max_loop_length)
        );

        // Store loop data
        loop_storage_out(loop, x) = loop_storage_in(
            clamp(loop, 0, loop_storage_in.dim(0).extent()-1),
            clamp(x, 0, max_loop_length - 1)
        );
        RDom rr(0, loop_storage_in.dim(0).extent()-1, 0, n_samples);
        loop_storage_out(rr.x, clamp(
                                    positions_in(rr.x) + rr.y,
                                    0,
                                    max_loop_length-1)
                                ) = select(
            state(rr.x) == Recording,
            samples_in(rr.x, rr.y),
            loop_storage_out(
                rr.x,
                clamp(positions_in(rr.x) + rr.y, 0, max_loop_length - 1)
            )
        );

        // Compute output samples
        samples_out(loop, x) = loop_storage_out(
            clamp(loop, 0, loop_storage_in.dim(0).extent()-1),
            clamp(pos(loop, x), 0, max_loop_length-1)
        );

        // Compute output states
        states_out(loop) = state(loop);

        // Compute output positions
        positions_out(loop) = pos(loop, n_samples-1);

        // Compute output lengths
        loop_lengths_out(loop) = len(loop);
        
        pos.compute_root()
           .reorder(loop, x)
           //.vectorize(loop, 8)
           ;
        pos.update(0)
           .reorder(loop, r)
           //.vectorize(loop, 8)
           ;
        loop_storage_out.compute_root()
            .reorder(loop, x)
            //.vectorize(loop, 8)
            ;
        loop_storage_out.update()
            .reorder(rr.x, rr.y)
            //.vectorize(rr.x, 8)
            ;

        samples_out.reorder(loop, x)
                   //.vectorize(loop, 8)
                   ;
        samples_out.print_loop_nest();
    }
};

}  // namespace

HALIDE_REGISTER_GENERATOR(Loops, shoopdaloop_loops)