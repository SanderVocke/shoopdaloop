#include <boost/ut.hpp>
#include <memory>
#include <functional>
#include <numeric>

using namespace boost::ut;

#define private public
#define protected public
#include "BasicLoop.h"
#undef private
#undef protected

suite BasicLoop_tests = []() {
    "basicloop_1_stop"_test = []() {
        BasicLoop loop;

        expect(loop.get_mode() == Stopped);
        expect(loop.PROC_get_next_poi() == std::nullopt);
        expect(loop.get_length() == 0);
        expect(loop.get_position() == 0);

        loop.PROC_process(1000);

        expect(loop.get_mode() == Stopped);
        expect(loop.PROC_get_next_poi() == std::nullopt);
        expect(loop.get_length() == 0);
        expect(loop.get_position() == 0);
    };

    "basicloop_2_record"_test = []() {
        BasicLoop loop;
        loop.set_mode(Recording, false);
        loop.PROC_update_poi();

        expect(loop.get_mode() == Recording);
        expect(loop.PROC_get_next_poi() == std::nullopt);
        expect(loop.get_length() == 0);
        expect(loop.get_position() == 0);

        loop.PROC_process(20);

        expect(loop.get_mode() == Recording);
        expect(loop.PROC_get_next_poi() == std::nullopt);
        expect(eq(loop.get_length(), 20));
        expect(eq(loop.get_position(), 0));
        
    };

    "basicloop_3_planned_transition"_test = []() {
        BasicLoop loop;
        auto other = std::make_shared<BasicLoop>();
        loop.set_soft_sync_source(other);
        loop.set_mode(Recording, false);
        loop.set_length(10, false);
        loop.PROC_update_poi();

        loop.plan_transition(Playing, 0);

        expect(eq(loop.PROC_get_next_poi().value_or(999), 999));
        expect(eq(loop.get_mode(), Recording));

        loop.PROC_trigger();

        expect(eq(loop.get_mode(), Playing));
        expect(eq(loop.PROC_get_next_poi().value_or(999), 10)); // End of loop
    };

    "basicloop_3_1_planned_transition_delayed"_test = []() {
        BasicLoop loop;
        auto other = std::make_shared<BasicLoop>();
        loop.set_soft_sync_source(other);
        loop.set_mode(Recording, false);
        loop.set_length(10, false);
        loop.PROC_update_poi();

        loop.plan_transition(Playing, 1);

        expect(eq(loop.PROC_get_next_poi().value_or(999), 999));
        expect(eq(loop.get_mode(), Recording));

        loop.PROC_trigger();

        expect(eq(loop.PROC_get_next_poi().value_or(999), 999));
        expect(eq(loop.get_mode(), Recording));

        loop.PROC_trigger();

        expect(eq(loop.get_mode(), Playing));
        expect(eq(loop.PROC_get_next_poi().value_or(999), 10)); // End of loop
    };

    "basicloop_3_2_planned_transitions_delayed"_test = []() {
        BasicLoop loop;
        auto other = std::make_shared<BasicLoop>();
        loop.set_soft_sync_source(other);
        loop.set_mode(Recording, false);
        loop.set_length(10, false);
        loop.PROC_update_poi();

        loop.plan_transition(Playing, 1);
        loop.plan_transition(Recording, 3);

        expect(eq(loop.PROC_get_next_poi().value_or(999), 999));
        expect(eq(loop.get_mode(), Recording));

        loop.PROC_trigger();

        expect(eq(loop.PROC_get_next_poi().value_or(999), 999));
        expect(eq(loop.get_mode(), Recording));

        loop.PROC_trigger();

        expect(eq(loop.get_mode(), Playing));
        expect(eq(loop.PROC_get_next_poi().value_or(999), 10)); // End of loop

        loop.PROC_trigger();

        expect(eq(loop.get_mode(), Playing));
        expect(eq(loop.PROC_get_next_poi().value_or(999), 10)); // End of loop

        loop.PROC_trigger();
        
        expect(eq(loop.PROC_get_next_poi().value_or(999), 999));
        expect(eq(loop.get_mode(), Recording));
    };

    "basicloop_3_3_planned_transitions_cancellation_1"_test = []() {
        BasicLoop loop;
        auto other = std::make_shared<BasicLoop>();
        loop.set_soft_sync_source(other);
        loop.set_mode(Recording, false);
        loop.set_length(10, false);
        loop.PROC_update_poi();

        loop.plan_transition(Playing, 3);
        loop.plan_transition(Stopped, 2);

        expect(eq(loop.PROC_get_next_poi().value_or(999), 999));
        expect(eq(loop.get_mode(), Recording));

        loop.PROC_trigger();
        loop.PROC_trigger();

        expect(eq(loop.PROC_get_next_poi().value_or(999), 999));
        expect(eq(loop.get_mode(), Recording));

        loop.PROC_trigger();

        expect(eq(loop.get_mode(), Stopped));
        expect(eq(loop.PROC_get_next_poi().value_or(999), 10)); // End of loop

        loop.PROC_trigger();
        loop.PROC_trigger();

        expect(eq(loop.get_mode(), Stopped));
        expect(eq(loop.PROC_get_next_poi().value_or(999), 10)); // End of loop
    };

    "basicloop_4_generate_trigger"_test = []() {
        BasicLoop loop;
        loop.set_mode(Stopped, false);
        loop.set_length(10, false);
        loop.set_position(0, false);
        
        expect(eq(loop.PROC_is_triggering_now(), false));
        loop.PROC_trigger();
        expect(eq(loop.PROC_is_triggering_now(), true));
    };

    "basicloop_4_1_generate_trigger_on_restart"_test = []() {
        BasicLoop loop;
        loop.set_length(10, false);
        loop.set_position(0, false);
        loop.set_mode(Playing, false);
        
        expect(eq(loop.PROC_is_triggering_now(), false));

        loop.PROC_update_poi();
        loop.PROC_process(9);

        expect(eq(loop.PROC_is_triggering_now(), false));

        loop.PROC_process(1);
        expect(eq(loop.PROC_is_triggering_now(), true));

        loop.PROC_handle_poi();

        expect(eq(loop.get_position(), 0));

        loop.PROC_process(5);

        expect(eq(loop.PROC_is_triggering_now(), false));
    };

    "basicloop_6_playback_0_length"_test = []() {
        BasicLoop loop;
        loop.set_mode(Playing, false);
        loop.set_length(0, false);
        loop.set_position(0, false);

        loop.PROC_update_poi();
        loop.PROC_process(10);

        expect(eq(loop.get_mode(), Stopped));
    };
};