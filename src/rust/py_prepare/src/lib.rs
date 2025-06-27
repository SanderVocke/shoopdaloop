use std::{env, path::PathBuf};

pub fn build_venv_dir() -> PathBuf {
    // If we're pre-building, don't do anything
    #[cfg(feature = "prebuild")]
    {
        return PathBuf::new();
    }

    #[cfg(not(feature = "prebuild"))]
    {
        let dir = env!("SHOOP_BUILD_VENV_DIR");
        let rval = PathBuf::from(dir);
        if !rval.exists() {
            panic!("SHOOP_BUILD_VENV_DIR {:?} does not exist", rval);
        }
        rval
    }
}

pub fn build_venv_python() -> String {
    // If we're pre-building, don't do anything
    #[cfg(feature = "prebuild")]
    {
        return String::new();
    }

    #[cfg(not(feature = "prebuild"))]
    {
        let rval = env!("SHOOP_BUILD_VENV_PYTHON");
        if rval.is_empty() {
            panic!("SHOOP_BUILD_VENV_PYTHON is empty");
        }
        rval.to_string()
    }
}
