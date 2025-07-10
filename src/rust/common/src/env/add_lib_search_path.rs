use std::env;
use std::path::Path;

pub fn add_lib_search_path(path: &Path) {
    let mut name: &str = "";
    if cfg!(target_os = "windows") {
        name = "PATH";
    } else if cfg!(target_os = "macos") {
        name = "DYLD_LIBRARY_PATH";
    } else if cfg!(target_os = "linux") {
        name = "LD_LIBRARY_PATH";
    }

    let value = env::var(name).unwrap_or("".to_string());
    env::set_var(
        name,
        format!(
            "{}{}{}",
            path.to_str().unwrap(),
            crate::util::PATH_LIST_SEPARATOR,
            value
        ),
    );
}
