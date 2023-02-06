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

        expect(loop.get_state() == Stopped);
        expect(loop.get_next_poi() == std::nullopt);
        expect(loop.get_length() == 0);
        expect(loop.get_position() == 0);

        loop.process(1000);

        expect(loop.get_state() == Stopped);
        expect(loop.get_next_poi() == std::nullopt);
        expect(loop.get_length() == 0);
        expect(loop.get_position() == 0);
    };

    "basicloop_2_record"_test = []() {
        BasicLoop loop;
        loop.m_state = Recording;
        loop.update_poi();

        expect(loop.get_state() == Recording);
        expect(loop.get_next_poi() == std::nullopt);
        expect(loop.get_length() == 0);
        expect(loop.get_position() == 0);

        loop.process(20);

        expect(loop.get_state() == Recording);
        expect(loop.get_next_poi() == std::nullopt);
        expect(eq(loop.get_length(), 20));
        expect(eq(loop.get_position(), 0));
        
    };

    "basicloop_3_planned_transition"_test = []() {
        BasicLoop loop;
        auto other = std::make_shared<BasicLoop>();
        loop.set_soft_sync_source(other);
        loop.m_state = Recording;
        loop.m_length = 10;
        loop.update_poi();

        loop.plan_transition(Playing, 0);

        expect(eq(loop.get_next_poi().value_or(999), 999));
        expect(eq(loop.get_state(), Recording));

        loop.trigger();

        expect(eq(loop.get_state(), Playing));
        expect(eq(loop.get_next_poi().value_or(999), 10)); // End of loop
    };

    "basicloop_3_1_planned_transition_delayed"_test = []() {
        BasicLoop loop;
        auto other = std::make_shared<BasicLoop>();
        loop.set_soft_sync_source(other);
        loop.m_state = Recording;
        loop.m_length = 10;
        loop.update_poi();

        loop.plan_transition(Playing, 1);

        expect(eq(loop.get_next_poi().value_or(999), 999));
        expect(eq(loop.get_state(), Recording));

        loop.trigger();

        expect(eq(loop.get_next_poi().value_or(999), 999));
        expect(eq(loop.get_state(), Recording));

        loop.trigger();

        expect(eq(loop.get_state(), Playing));
        expect(eq(loop.get_next_poi().value_or(999), 10)); // End of loop
    };

    "basicloop_3_2_planned_transitions_delayed"_test = []() {
        BasicLoop loop;
        auto other = std::make_shared<BasicLoop>();
        loop.set_soft_sync_source(other);
        loop.m_state = Recording;
        loop.m_length = 10;
        loop.update_poi();

        loop.plan_transition(Playing, 1);
        loop.plan_transition(Recording, 1);

        expect(eq(loop.get_next_poi().value_or(999), 999));
        expect(eq(loop.get_state(), Recording));

        loop.trigger();

        expect(eq(loop.get_next_poi().value_or(999), 999));
        expect(eq(loop.get_state(), Recording));

        loop.trigger();

        expect(eq(loop.get_state(), Playing));
        expect(eq(loop.get_next_poi().value_or(999), 10)); // End of loop

        loop.trigger();

        expect(eq(loop.get_state(), Playing));
        expect(eq(loop.get_next_poi().value_or(999), 10)); // End of loop

        loop.trigger();
        
        expect(eq(loop.get_next_poi().value_or(999), 999));
        expect(eq(loop.get_state(), Recording));
    };

    "basicloop_4_generate_trigger"_test = []() {
        BasicLoop loop;
        loop.m_state = Stopped;
        loop.m_length = 10;
        loop.m_position = 0;
        
        expect(eq(loop.is_triggering_now(), false));
        loop.trigger();
        expect(eq(loop.is_triggering_now(), true));
    };

    "basicloop_4_1_generate_trigger_on_restart"_test = []() {
        BasicLoop loop;
        loop.m_state = Playing;
        loop.m_length = 10;
        loop.m_position = 0;
        
        expect(eq(loop.is_triggering_now(), false));

        loop.update_poi();
        loop.process(9);

        expect(eq(loop.is_triggering_now(), false));

        loop.process(1);
        expect(eq(loop.is_triggering_now(), true));

        loop.handle_poi();

        expect(eq(loop.get_position(), 0));

        loop.process(5);

        expect(eq(loop.is_triggering_now(), false));
    };

    "basicloop_6_playback_0_length"_test = []() {
        BasicLoop loop;
        loop.m_state = Playing;
        loop.m_length = 0;
        loop.m_position = 0;

        loop.update_poi();
        loop.process(10);

        expect(eq(loop.get_state(), Stopped));
    };
};