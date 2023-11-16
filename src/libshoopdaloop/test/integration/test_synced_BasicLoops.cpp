#define BASICLOOP_EXPOSE_ALL_FOR_TEST
#include "BasicLoop.h"

#include <catch2/catch_test_macros.hpp>
#include <memory>
#include <functional>
#include <numeric>
#include "process_loops.h"

TEST_CASE("SyncedBasicLoops - Simple", "[SyncedBasicLoops]") {
    auto loop1 = std::make_shared<BasicLoop>();
    auto loop2 = std::make_shared<BasicLoop>();
    
    loop1->set_sync_source(loop2);
    loop1->set_mode(Stopped, false);
    loop1->set_length(10, false);
    loop1->plan_transition(Playing);

    loop2->PROC_trigger();
    REQUIRE(loop2->PROC_is_triggering_now() == true);

    loop1->PROC_handle_sync();

    REQUIRE(loop1->get_mode() == Playing);
};

TEST_CASE("SyncedBasicLoops - Loop Restart", "[SyncedBasicLoops]") {
    auto loop1 = std::make_shared<BasicLoop>();
    auto loop2 = std::make_shared<BasicLoop>();

    loop1->set_sync_source(loop2);
    loop1->set_length(100, false);
    loop1->set_mode(Stopped, false);
    loop1->plan_transition(Playing);
    loop1->PROC_update_poi();

    loop2->set_length(100, false);
    loop2->set_mode(Playing);
    loop2->set_position(90, false);
    loop2->PROC_update_poi();

    process_loops<BasicLoop>({loop1, loop2}, 20);

    REQUIRE(loop2->get_position() == 10);
    REQUIRE(loop2->get_mode() == Playing);
    REQUIRE(loop2->PROC_is_triggering_now() == false);

    REQUIRE(loop1->get_mode() == Playing);
    REQUIRE(loop1->get_position() == 10);
};

TEST_CASE("SyncedBasicLoops - Loop Restart Playing", "[SyncedBasicLoops]") {
    auto loop1 = std::make_shared<BasicLoop>();
    auto loop2 = std::make_shared<BasicLoop>();

    loop1->set_sync_source(loop2);
    loop1->set_length(100, false);
    loop1->set_mode(Playing, false);
    loop1->set_position(90, false);
    loop1->PROC_update_poi();

    loop2->set_length(150, false);
    loop2->set_mode(Playing);
    loop2->set_position(90, false);
    loop2->PROC_update_poi();

    process_loops<BasicLoop>({loop1, loop2}, 20);

    REQUIRE(loop2->get_position() == 110);
    REQUIRE(loop2->get_mode() == Playing);
    REQUIRE(loop2->PROC_is_triggering_now() == false);

    REQUIRE(loop1->get_mode() == Playing);
    REQUIRE(loop1->get_position() == 100);

    process_loops<BasicLoop>({loop1, loop2}, 50);

    REQUIRE(loop2->get_position() == 10);
    REQUIRE(loop2->get_mode() == Playing);
    REQUIRE(loop2->PROC_is_triggering_now() == false);

    REQUIRE(loop1->get_mode() == Playing);
    REQUIRE(loop1->get_position() == 10);
};