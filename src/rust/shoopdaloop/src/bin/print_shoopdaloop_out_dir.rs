const OUT_DIR: Option<&str> = option_env!("OUT_DIR");

fn main() {
    match OUT_DIR {
        Some(dir) => println!("{}", dir),
        None => eprintln!("OUT_DIR variable not set"),
    }
}
