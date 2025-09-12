fn main() {
    cxx_build::bridge("src/cxx.rs")
        .std("c++14")
        .compile("refilling-pool");

    println!("cargo:rerun-if-changed=src/cxx.rs");
}
