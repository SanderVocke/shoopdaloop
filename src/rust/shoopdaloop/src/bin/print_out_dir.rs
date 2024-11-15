const SHOOP_BUILD_OUT_DIR : Option<&str> = option_env!("OUT_DIR");

fn main() {
    println!("{}", SHOOP_BUILD_OUT_DIR.unwrap());
}