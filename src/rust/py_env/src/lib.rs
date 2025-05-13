use std::path::PathBuf;

// pub fn py_env_dir() -> PathBuf {
//     // If we're pre-building, don't do anything
//     #[cfg(feature = "prebuild")]
//     {
//         return PathBuf::new();
//     }

//     #[cfg(not(feature = "prebuild"))]
//     {
//         let dir = env!("SHOOP_PY_ENV_DIR");
//         let rval = PathBuf::from(dir);
//         if !rval.exists() {
//             panic!("SHOOP_PY_ENV_DIR does not exist");
//         }
//         rval
//     }
// }

pub fn shoopdaloop_wheel() -> PathBuf {
    // If we're pre-building, don't do anything
    #[cfg(feature = "prebuild")]
    {
        return PathBuf::new();
    }

    #[cfg(not(feature = "prebuild"))]
    {
        let dir = env!("SHOOPDALOOP_WHEEL");
        let rval = PathBuf::from(dir);
        if !rval.exists() {
            panic!("SHOOPDALOOP_WHEEL {} does not exist", rval);
        }
        rval
    }
}

pub fn dev_venv_dir() -> PathBuf {
    // If we're pre-building, don't do anything
    #[cfg(feature = "prebuild")]
    {
        return PathBuf::new();
    }

    #[cfg(not(feature = "prebuild"))]
    {
        let dir = env!("SHOOP_DEV_VENV_DIR");
        let rval = PathBuf::from(dir);
        if !rval.exists() {
            panic!("SHOOP_DEV_VENV_DIR {} does not exist", rval);
        }
        rval
    }
}