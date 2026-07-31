#[cfg(all(not(feature = "prebuild"), debug_assertions))]
#[global_allocator]
static REALTIME_ALLOC_GUARD_ALLOCATOR: assert_no_alloc::AllocDisabler =
    assert_no_alloc::AllocDisabler;

#[cfg(not(feature = "prebuild"))]
mod main_impl;

#[cfg(not(feature = "prebuild"))]
fn main() {
    use main_impl::main;
    main();
}

#[cfg(feature = "prebuild")]
fn main() {}
