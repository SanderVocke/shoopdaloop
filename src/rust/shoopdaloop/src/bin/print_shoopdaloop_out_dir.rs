const OUT_DIR: Option<&str> = option_env!("OUT_DIR");

fn main() {
    println!("{}", OUT_DIR.unwrap_or("(not found)"));
}
