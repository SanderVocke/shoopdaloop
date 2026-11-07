#[cfg(unix)]
#[path = "zynq/mod.rs"]
mod probe;

fn main() {
    #[cfg(unix)]
    probe::run();
    #[cfg(not(unix))]
    panic!("The Zynq QEMU probe requires a Unix host");
}
