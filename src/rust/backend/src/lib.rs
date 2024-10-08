use std::path::PathBuf;

pub fn backend_build_dir() -> PathBuf {
    // If we're pre-building, return an invalid path
    #[cfg(feature = "prebuild")]
    {
        return PathBuf::new();
    }

    #[cfg(not(feature = "prebuild"))]
    {
        let build_dir = env!("SHOOP_BACKEND_DIR");
        let rval = PathBuf::from(build_dir);
        if !rval.exists() {
            panic!("SHOOP_BACKEND_DIR does not exist");
        }
        rval
    }
}