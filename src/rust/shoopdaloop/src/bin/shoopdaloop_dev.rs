#[cfg(not(feature = "prebuild"))]
fn main() {
    use shoopdaloop::shoopdaloop_dev_impl::main;
    main();
}

#[cfg(feature = "prebuild")]
fn main() {}