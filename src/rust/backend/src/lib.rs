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

pub fn backend_qt_qmake_path() -> PathBuf {
    // If we're pre-building, return an invalid path
    #[cfg(feature = "prebuild")]
    {
        return PathBuf::new();
    }
    #[cfg(not(feature = "prebuild"))]
    {
        let qmake_path = env!("SHOOP_QT_QMAKE_PATH");
        let rval = PathBuf::from(qmake_path);
        if !rval.exists() {
            panic!("SHOOP_QT_QMAKE_PATH does not exist");
        }
        rval
    }
}