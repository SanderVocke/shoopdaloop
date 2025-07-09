pub const PATH_LIST_SEPARATOR: &'static str = if cfg!(target_os = "windows") {
    ";"
} else {
    ":"
};
