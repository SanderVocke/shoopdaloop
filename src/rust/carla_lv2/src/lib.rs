// carla_lv2 — Double-bridge crate for Carla LV2 processing chain.
//
// Architecture:
//   Inner bridge (extern "C++") — Rust calls C++ CarlaLv2ChainCppFacade.
//   Outer bridge (extern "Rust") — C++ calls Rust FxChain.
//   FxChain delegates everything to the inner bridge.
//
// TODO: Enable when the bridge modules are implemented.
// pub mod inner_bridge;
// pub mod outer_bridge;
// mod fx_chain;
