use std::path::Path;
use std::env;

pub fn add_lib_search_path(path : &Path) {
    let path_separator = if cfg!(target_os = "windows") { ";" } else { ":" };
    let mut name : &str = "";
    if cfg!(target_os = "windows") {
        name = "PATH";
    } else if cfg!(target_os = "macos") {
        name = "DYLD_LIBRARY_PATH";
    } else if cfg!(target_os = "linux") {
        name = "LD_LIBRARY_PATH";
    }

    let value = env::var(name).unwrap_or("".to_string());
    env::set_var(name, format!("{}{}{}", path.to_str().unwrap(), path_separator, value));
    println!("{} => {}", name, env::var(name).unwrap());
}