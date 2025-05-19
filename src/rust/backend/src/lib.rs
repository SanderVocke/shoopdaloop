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

pub fn backend_link_dir() -> PathBuf {
    // If we're pre-building, return an invalid path
    #[cfg(feature = "prebuild")]
    {
        return PathBuf::new();
    }

    #[cfg(not(feature = "prebuild"))]
    {
        let build_dir = backend_build_dir();
        if cfg!(debug_assertions) {
            return build_dir.join("debug").join("lib");
        } else {
            return build_dir.join("lib");
        }
    }
}

pub fn zita_link_dir() -> PathBuf {
    #[cfg(feature = "prebuild")]
    {
        return PathBuf::new();
    }

    #[cfg(not(feature = "prebuild"))]
    {
        let build_dir = env!("SHOOP_ZITA_LINK_DIR");
        let rval = PathBuf::from(build_dir);
        if !rval.exists() {
            panic!("SHOOP_ZITA_LINK_DIR does not exist");
        }
        rval
    }
}

pub fn all_link_search_paths() -> Vec<PathBuf> {
    // If we're pre-building, return an empty vector
    #[cfg(feature = "prebuild")]
    {
        return vec![];
    }

    #[cfg(not(feature = "prebuild"))]
    {
        let mut rval = vec![];
        rval.push(backend_link_dir());
        rval.push(zita_link_dir());
        rval
    }
}