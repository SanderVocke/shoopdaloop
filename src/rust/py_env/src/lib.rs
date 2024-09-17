use std::path::PathBuf;

pub fn py_env_dir() -> PathBuf {
    let dir = env!("SHOOP_PY_ENV_DIR");
    let rval = PathBuf::from(dir);
    if !rval.exists() {
        panic!("SHOOP_PY_ENV_DIR does not exist");
    }
    rval
}

pub fn py_interpreter() -> PathBuf {
    let i = env!("SHOOP_PY_INTERPRETER");
    let rval = PathBuf::from(i);
    if !rval.exists() {
        panic!("SHOOP_PY_INTERPRETER does not exist");
    }
    rval
}