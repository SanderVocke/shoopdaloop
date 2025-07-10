use backend;

fn main() {
    println!("backend_build_dir: {:?}", backend::backend_build_dir());
    println!(
        "backend_build_time_link_dir: {:?}",
        backend::backend_build_time_link_dir()
    );
    println!(
        "backend_runtime_link_dir: {:?}",
        backend::backend_runtime_link_dir()
    );
    println!(
        "build_time_link_dirs: {:?}",
        backend::build_time_link_dirs()
    );
    println!("runtime_link_dirs: {:?}", backend::runtime_link_dirs());
}
