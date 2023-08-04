#include <boost/ut.hpp>
#include "libshoopdaloop_test_if.h"
#include "libshoopdaloop.h"
#include <memory>
#include "types.h"

using namespace boost::ut;

suite libshoopdaloop_if_tests = []() {
    "lif_1_create_destroy_backend"_test = []() {
        shoopdaloop_backend_instance_t * c_backend = initialize (Dummy, "dummy");
        auto weak_backend = std::weak_ptr<Backend>(internal_backend(c_backend));
        expect(neq(weak_backend.lock(), nullptr));
        terminate(c_backend);
        expect(eq(weak_backend.lock(), nullptr));
    };

    "lif_2_create_destroy_loop"_test = []() {
        shoopdaloop_backend_instance_t * c_backend = initialize (Dummy, "dummy");
        {
            auto c_loop = create_loop(c_backend);
            auto weak_loop = std::weak_ptr<ConnectedLoop>(internal_loop(c_loop));
            expect(neq(weak_loop.lock(), nullptr));
            destroy_loop(c_loop);
            expect(eq(weak_loop.lock(), nullptr));
        }
        terminate(c_backend);
    };

    "lif_3_create_destroy_backend_loop_destroyed"_test = []() {
        shoopdaloop_backend_instance_t * c_backend = initialize (Dummy, "dummy");
        {
            auto c_loop = create_loop(c_backend);
            auto weak_loop = std::weak_ptr<ConnectedLoop>(internal_loop(c_loop));
            expect(neq(weak_loop.lock(), nullptr));
            terminate(c_backend);
            expect(eq(weak_loop.lock(), nullptr));
        }
    };
};