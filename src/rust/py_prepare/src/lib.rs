use std::path::PathBuf;

pub fn build_venv_dir() -> PathBuf {
    // If we're pre-building, don't do anything
    if cfg!(feature = "prebuild") {
        return PathBuf::new();
    }

    let dir = option_env!("SHOOP_BUILD_VENV_DIR").unwrap();
    let rval = PathBuf::from(dir);
    if !rval.exists() {
        panic!("SHOOP_BUILD_VENV_DIR {:?} does not exist", rval);
    }
    rval
}

pub fn build_venv_python() -> String {
    // If we're pre-building, don't do anything
    if cfg!(feature = "prebuild") {
        return String::new();
    }

    let rval = option_env!("SHOOP_BUILD_VENV_PYTHON").unwrap();
    if rval.is_empty() {
        panic!("SHOOP_BUILD_VENV_PYTHON is empty");
    }
    rval.to_string()
}
