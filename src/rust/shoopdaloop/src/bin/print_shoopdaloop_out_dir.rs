#[cfg(not(feature = "prebuild"))]
const OUT_DIR: Option<&str> = option_env!("OUT_DIR");

#[cfg(not(feature = "prebuild"))]
fn main() {
    match OUT_DIR {
        Some(dir) => println!("{}", dir),
        None => eprintln!("OUT_DIR variable not set"),
    }
}

#[cfg(feature = "prebuild")]
fn main() {}
