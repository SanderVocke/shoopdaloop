#include <catch2/catch_test_macros.hpp>

#include "backend_rust/src/rust_bridge_object_test_cxx.rs.h"

#include <optional>
#include <utility>

TEST_CASE("RustBridgeObject - strong downgrades and weak upgrades", "[RustBridgeObject]") {
    backend_rust::test_rust_bridge_object_reset_drop_count();

    {
        auto strong = backend_rust::new_test_rust_bridge_object();
        auto weak = strong->downgrade();

        REQUIRE(strong->valid());
        REQUIRE(weak->valid());
        CHECK(strong->id() == weak->id());
        CHECK(strong->strong_count() == 1);
        CHECK(weak->strong_count() == 1);

        auto upgraded = weak->upgrade();
        REQUIRE(upgraded->valid());
        CHECK(upgraded->id() == strong->id());
        CHECK(weak->strong_count() == 2);

        CHECK(backend_rust::test_rust_bridge_object_increment(*weak, 5));
        CHECK(backend_rust::test_rust_bridge_object_value(*weak) == 5);

        auto weak_clone = weak->clone();
        REQUIRE(weak_clone->valid());
        CHECK(weak_clone->id() == weak->id());
        CHECK(backend_rust::test_rust_bridge_object_increment(*weak_clone, 2));
        CHECK(backend_rust::test_rust_bridge_object_value(*weak) == 7);
    }

    CHECK(backend_rust::test_rust_bridge_object_drop_count() == 1);
}

TEST_CASE("RustBridgeObject - weak expires when strong handles are gone", "[RustBridgeObject]") {
    backend_rust::test_rust_bridge_object_reset_drop_count();

    auto weak = []() {
        auto strong = backend_rust::new_test_rust_bridge_object();
        auto weak = strong->downgrade();
        REQUIRE(weak->valid());
        CHECK(backend_rust::test_rust_bridge_object_increment(*weak, 3));
        return weak;
    }();

    CHECK(backend_rust::test_rust_bridge_object_drop_count() == 1);
    CHECK(!weak->valid());
    CHECK(!backend_rust::test_rust_bridge_object_increment(*weak, 1));
    CHECK(backend_rust::test_rust_bridge_object_value(*weak) == 0);

    auto upgraded = weak->upgrade();
    CHECK(!upgraded->valid());
    CHECK(upgraded->id() == weak->id());
    CHECK(upgraded->strong_count() == 0);
}

TEST_CASE("RustBridgeObject - cloned strong keeps Rust object alive", "[RustBridgeObject]") {
    backend_rust::test_rust_bridge_object_reset_drop_count();

    std::optional<rust::Box<backend_rust::TestRustBridgeObjectBridgeStrong>> keeper;

    auto weak = [&keeper]() {
        auto strong = backend_rust::new_test_rust_bridge_object();
        auto weak = strong->downgrade();
        auto clone = strong->clone_strong();
        CHECK(weak->strong_count() == 2);
        keeper.emplace(std::move(clone));
        return weak;
    }();

    CHECK(backend_rust::test_rust_bridge_object_drop_count() == 0);
    REQUIRE(weak->valid());
    CHECK(weak->strong_count() == 1);
    CHECK(backend_rust::test_rust_bridge_object_increment(*weak, 9));
    CHECK(backend_rust::test_rust_bridge_object_value(*weak) == 9);

    keeper.reset();

    CHECK(backend_rust::test_rust_bridge_object_drop_count() == 1);
    CHECK(!weak->valid());
    CHECK(!backend_rust::test_rust_bridge_object_increment(*weak, 1));
}
