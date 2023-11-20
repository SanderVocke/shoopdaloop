
#define BASICLOOP_EXPOSE_ALL_FOR_TEST
#include "BasicLoop.h"

#include <catch2/catch_test_macros.hpp>
#include <memory>
#include <functional>
#include <numeric>
#include <iostream>

TEST_CASE("BasicLoop - Stop", "[BasicLoop]") {
    BasicLoop loop;

    REQUIRE(loop.get_mode() == LoopMode_Stopped);
    REQUIRE(loop.PROC_get_next_poi() == std::nullopt);
    REQUIRE(loop.get_length() == 0);
    REQUIRE(loop.get_position() == 0);

    loop.PROC_process(1000);

    REQUIRE(loop.get_mode() == LoopMode_Stopped);
    REQUIRE(loop.PROC_get_next_poi() == std::nullopt);
    REQUIRE(loop.get_length() == 0);
    REQUIRE(loop.get_position() == 0);
};

TEST_CASE("BasicLoop - Record", "[BasicLoop]") {
    BasicLoop loop;
    loop.set_mode(LoopMode_Recording, false);
    loop.PROC_update_poi();

    REQUIRE(loop.get_mode() == LoopMode_Recording);
    REQUIRE(loop.PROC_get_next_poi() == std::nullopt);
    REQUIRE(loop.get_length() == 0);
    REQUIRE(loop.get_position() == 0);

    loop.PROC_process(20);

    REQUIRE(loop.get_mode() == LoopMode_Recording);
    REQUIRE(loop.PROC_get_next_poi() == std::nullopt);
    REQUIRE(loop.get_length() == 20);
    REQUIRE(loop.get_position() == 0);
    
};

TEST_CASE("BasicLoop - Planned Transition", "[BasicLoop]") {
    BasicLoop loop;
    auto other = std::make_shared<BasicLoop>();
    loop.set_sync_source(other);
    loop.set_mode(LoopMode_Recording, false);
    loop.set_length(10, false);
    loop.PROC_update_poi();

    loop.plan_transition(LoopMode_Playing, 0);

    REQUIRE(loop.PROC_get_next_poi().value_or(999) == 999);
    REQUIRE(loop.get_mode() == LoopMode_Recording);

    loop.PROC_trigger();

    REQUIRE(loop.get_mode() == LoopMode_Playing);
    REQUIRE(loop.PROC_get_next_poi().value_or(999) == 10); // End of loop
};

TEST_CASE("BasicLoop - Planned Transition delayed", "[BasicLoop]") {
    BasicLoop loop;
    auto other = std::make_shared<BasicLoop>();
    loop.set_sync_source(other);
    loop.set_mode(LoopMode_Recording, false);
    loop.set_length(10, false);
    loop.PROC_update_poi();

    loop.plan_transition(LoopMode_Playing, 1);

    REQUIRE(loop.PROC_get_next_poi().value_or(999) == 999);
    REQUIRE(loop.get_mode() == LoopMode_Recording);

    loop.PROC_trigger();
    loop.PROC_process(1); // Cannot trigger twice in same cycle

    REQUIRE(loop.PROC_get_next_poi().value_or(999) == 999);
    REQUIRE(loop.get_mode() == LoopMode_Recording);

    loop.PROC_trigger();

    REQUIRE(loop.get_mode() == LoopMode_Playing);
    REQUIRE(loop.PROC_get_next_poi().value_or(999) == 11); // End of loop
};

TEST_CASE("BasicLoop - Planned Transitions delayed", "[BasicLoop]") {
    BasicLoop loop;
    auto other = std::make_shared<BasicLoop>();
    loop.set_sync_source(other);
    loop.set_mode(LoopMode_Recording, false);
    loop.set_length(10, false);
    loop.PROC_update_poi();

    loop.plan_transition(LoopMode_Playing, 1);
    loop.plan_transition(LoopMode_Recording, 3);

    REQUIRE(loop.PROC_get_next_poi().value_or(999) == 999);
    REQUIRE(loop.get_mode() == LoopMode_Recording);

    loop.PROC_trigger();

    REQUIRE(loop.PROC_get_next_poi().value_or(999) == 999);
    REQUIRE(loop.get_mode() == LoopMode_Recording);

    loop.PROC_process(1); // Cannot trigger twice in same cycle
    loop.PROC_trigger();

    REQUIRE(loop.get_mode() == LoopMode_Playing);
    REQUIRE(loop.PROC_get_next_poi().value_or(999) == 11); // End of loop

    loop.PROC_process(1); // Cannot trigger twice in same cycle
    loop.PROC_trigger();

    REQUIRE(loop.get_mode() == LoopMode_Playing);
    REQUIRE(loop.PROC_get_next_poi().value_or(999) == 10); // End of loop

    loop.PROC_process(1); // Cannot trigger twice in same cycle
    loop.PROC_trigger();
    
    REQUIRE(loop.PROC_get_next_poi().value_or(999) == 999);
    REQUIRE(loop.get_mode() == LoopMode_Recording);
};

TEST_CASE("BasicLoop - Planned Transitions Cancellation 1", "[BasicLoop]") {
    BasicLoop loop;
    auto other = std::make_shared<BasicLoop>();
    loop.set_sync_source(other);
    loop.set_mode(LoopMode_Recording, false);
    loop.set_length(10, false);
    loop.PROC_update_poi();

    loop.plan_transition(LoopMode_Playing, 3);
    loop.plan_transition(LoopMode_Stopped, 2);

    REQUIRE(loop.PROC_get_next_poi().value_or(999) == 999);
    REQUIRE(loop.get_mode() == LoopMode_Recording);

    loop.PROC_trigger();
    loop.PROC_process(1); // Cannot trigger twice in same cycle
    loop.PROC_trigger();

    REQUIRE(loop.PROC_get_next_poi().value_or(999) == 999);
    REQUIRE(loop.get_mode() == LoopMode_Recording);

    loop.PROC_process(1); // Cannot trigger twice in same cycle
    loop.PROC_trigger();

    REQUIRE(loop.get_mode() == LoopMode_Stopped);
    REQUIRE(loop.PROC_get_next_poi().value_or(999) == 999);

    loop.PROC_process(1); // Cannot trigger twice in same cycle
    loop.PROC_trigger();
    loop.PROC_process(1); // Cannot trigger twice in same cycle
    loop.PROC_trigger();

    REQUIRE(loop.get_mode() == LoopMode_Stopped);
    REQUIRE(loop.PROC_get_next_poi().value_or(999) == 999);
};

TEST_CASE("BasicLoop - Generate Trigger", "[BasicLoop]") {
    BasicLoop loop;
    loop.set_mode(LoopMode_Stopped, false);
    loop.set_length(10, false);
    loop.set_position(0, false);
    
    REQUIRE(loop.PROC_is_triggering_now() == false);
    loop.PROC_trigger();
    REQUIRE(loop.PROC_is_triggering_now() == true);
};

TEST_CASE("BasicLoop - Generate Trigger on restart", "[BasicLoop]") {
    BasicLoop loop;
    REQUIRE(loop.PROC_is_triggering_now() == false);

    loop.set_length(10, false);
    loop.set_mode(LoopMode_Playing, false);
    loop.PROC_process(1);
    
    REQUIRE(loop.PROC_is_triggering_now() == false);

    loop.PROC_update_poi();
    loop.PROC_process(8);

    REQUIRE(loop.PROC_is_triggering_now() == false);

    loop.PROC_process(1);
    REQUIRE(loop.PROC_is_triggering_now() == true);

    loop.PROC_handle_poi();

    REQUIRE(loop.get_position() == 0);

    loop.PROC_process(5);

    REQUIRE(loop.PROC_is_triggering_now() == false);
};

TEST_CASE("BasicLoop - Playback 0 length", "[BasicLoop]") {
    BasicLoop loop;
    loop.set_mode(LoopMode_Playing, false);
    loop.set_length(0, false);
    loop.set_position(0, false);

    loop.PROC_update_poi();
    loop.PROC_process(10);

    REQUIRE(loop.get_mode() == LoopMode_Stopped);
};