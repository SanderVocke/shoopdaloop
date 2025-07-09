#[cfg(not(feature = "prebuild"))]
mod main_impl;

#[cfg(not(feature = "prebuild"))]
fn main() {
    use main_impl::main;
    main();
}

#[cfg(feature = "prebuild")]
fn main() {}
