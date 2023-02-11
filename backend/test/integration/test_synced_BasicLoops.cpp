#include <boost/ut.hpp>
#include <memory>
#include <functional>
#include <numeric>
#include "process_loops.h"

using namespace boost::ut;

#define private public
#define protected public
#include "BasicLoop.h"
#undef private
#undef protected

suite Synced_BasicLoops_tests = []() {
    "bl_sync_1_simple"_test = []() {
        auto loop1 = std::make_shared<BasicLoop>();
        auto loop2 = std::make_shared<BasicLoop>();
        
        loop1->set_soft_sync_source(loop2);
        loop1->m_state = Stopped;
        loop1->set_length(10);
        loop1->plan_transition(Playing);

        loop2->trigger();
        expect(eq(loop2->is_triggering_now(), true));

        loop1->handle_soft_sync();

        expect(eq(loop1->get_state(), Playing));
    };

    "bl_sync_2_loop_restart"_test = []() {
        auto loop1 = std::make_shared<BasicLoop>();
        auto loop2 = std::make_shared<BasicLoop>();

        loop1->set_soft_sync_source(loop2);
        loop1->m_state = Stopped;
        loop1->m_length = 100;
        loop1->plan_transition(Playing);
        loop1->update_poi();

        loop2->m_state = Playing;
        loop2->m_length = 100;
        loop2->m_position = 90;
        loop2->update_poi();

        process_loops<BasicLoop>({loop1, loop2}, 20);

        expect(eq(loop2->get_position(), 10));
        expect(eq(loop2->get_state(), Playing));
        expect(eq(loop2->is_triggering_now(), false));

        expect(eq(loop1->get_state(), Playing));
        expect(eq(loop1->get_position(), 10));
    };
};