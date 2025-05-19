use std::path::PathBuf;

pub fn remove_subpaths(paths : &Vec<String>) -> Vec<String> {
    let mut out : Vec<String> = vec![];
    for path in paths.iter().map(|p| PathBuf::from(p)) {
        let mut include = true;
        for other_path in paths.iter().map(|p| PathBuf::from(p)) {
            if path == other_path { continue; }
            if path.starts_with(other_path) { include = false; break; }
        }
        if include { out.push(path.to_string_lossy().into_owned()); }
    }
    return out;
}