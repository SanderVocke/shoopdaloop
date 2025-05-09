const SHOOP_RUNTIME_ENV_DIR : Option<&str> = option_env!("SHOOP_RUNTIME_ENV_DIR");

fn main() {
    println!("{}", SHOOP_RUNTIME_ENV_DIR.unwrap());
}