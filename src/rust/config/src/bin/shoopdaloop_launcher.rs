fn main() -> std::io::Result<()> {
    common::init().unwrap();
    config::launcher::launcher()
}