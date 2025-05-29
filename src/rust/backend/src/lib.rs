use std::path::PathBuf;
use common;

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

pub fn backend_build_time_link_dir() -> PathBuf {
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

pub fn backend_runtime_link_dir() -> PathBuf {
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

pub fn build_time_link_dirs() -> Vec<PathBuf> {
    // If we're pre-building, return an empty vector
    #[cfg(feature = "prebuild")]
    {
        return vec![];
    }

    #[cfg(not(feature = "prebuild"))]
    {
        let build_time_link_dirs =
             option_env!("SHOOP_BACKEND_BUILD_TIME_LINK_DIRS")
            .unwrap_or_default()
            .split(common::fs::PATH_LIST_SEPARATOR)
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .collect::<Vec<_>>();

        build_time_link_dirs
    }
}

pub fn runtime_link_dirs() -> Vec<PathBuf> {
    // If we're pre-building, return an empty vector
    #[cfg(feature = "prebuild")]
    {
        return vec![];
    }

    #[cfg(not(feature = "prebuild"))]
    {
        let mut runtime_link_dirs =
             option_env!("SHOOP_BACKEND_RUNTIME_LINK_DIRS")
            .unwrap_or_default()
            .split(common::fs::PATH_LIST_SEPARATOR)
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .collect::<Vec<_>>();
        runtime_link_dirs.push(backend_runtime_link_dir());

        runtime_link_dirs
    }
}