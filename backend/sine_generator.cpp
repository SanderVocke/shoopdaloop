#include "Halide.h"
#include "types.hpp"

namespace {

class Sine : public Halide::Generator<Sine> {
public:
    Input<int32_t> sample_rate{"sample_rate"};
    Input<float> frequency{"frequency"};
    Output<Buffer<float, 2>> out{"out"};

    void generate() {
        Var x("x"), y("y");
        out(x, y) = Halide::sin(Halide::cast<float>(x) / Halide::cast<float>(sample_rate) * 2.0f * 3.14f * frequency);
    }
};

}  // namespace

HALIDE_REGISTER_GENERATOR(Sine, sine)