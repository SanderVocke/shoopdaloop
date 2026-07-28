#[cfg(not(feature = "prebuild"))]
const OUT_DIR: &str = env!("OUT_DIR");

#[cfg(not(feature = "prebuild"))]
fn main() {
    println!("{}", OUT_DIR);
}

#[cfg(feature = "prebuild")]
fn main() {}
